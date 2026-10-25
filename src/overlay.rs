//! A click-through, per-pixel-alpha Win32 layer. Coordinates are physical pixels.
use crate::model::Settings;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
pub struct DesktopEffects {
    config: Arc<Mutex<Option<Settings>>>,
    error: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl DesktopEffects {
    pub fn new(emergency: Arc<AtomicBool>) -> Self {
        let config = Arc::new(Mutex::new(None));
        let error = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let c = config.clone();
        let e = error.clone();
        let s = stop.clone();
        let worker = std::thread::spawn(move || run(c, e, s, emergency));
        Self {
            config,
            error,
            stop,
            worker: Some(worker),
        }
    }
    pub fn set(&self, config: Option<Settings>) {
        if let Ok(mut v) = self.config.lock() {
            *v = config;
        }
    }
    pub fn take_error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|mut v| v.take())
    }
}
impl Drop for DesktopEffects {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
#[cfg(not(windows))]
fn run(_: Arc<Mutex<Option<Settings>>>, _: Arc<Mutex<Option<String>>>, stop: Arc<AtomicBool>) {
    let _ = emergency;
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
}
#[cfg(windows)]
fn run(
    config: Arc<Mutex<Option<Settings>>>,
    error: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
    emergency: Arc<AtomicBool>,
) {
    if let Err(e) = win::run(config, stop, emergency)
        && let Ok(mut v) = error.lock()
    {
        *v = Some(e);
    }
}
#[cfg(windows)]
mod win {
    use super::*;
    use std::{
        collections::VecDeque,
        ptr,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{HWND, POINT, SIZE},
        Graphics::Gdi::*,
        UI::WindowsAndMessaging::*,
    };
    const N: usize = 640;
    struct Layer {
        hwnd: HWND,
        dc: HDC,
        bitmap: HBITMAP,
        old: HGDIOBJ,
        bits: *mut u8,
    }
    impl Layer {
        fn new() -> Result<Self, String> {
            // SAFETY: STATIC is a system window class. No custom callback or borrowed data is retained.
            // The window and GDI objects are owned and used only by this worker thread.
            unsafe {
                let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
                let title: Vec<u16> = "Zendaya Cursor · Effets\0".encode_utf16().collect();
                let hwnd = CreateWindowExW(
                    WS_EX_LAYERED
                        | WS_EX_TRANSPARENT
                        | WS_EX_TOOLWINDOW
                        | WS_EX_NOACTIVATE
                        | WS_EX_TOPMOST,
                    class.as_ptr(),
                    title.as_ptr(),
                    WS_POPUP,
                    0,
                    0,
                    N as i32,
                    N as i32,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null(),
                );
                if hwnd.is_null() {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                let dc = CreateCompatibleDC(ptr::null_mut());
                if dc.is_null() {
                    DestroyWindow(hwnd);
                    return Err("Création du contexte des effets impossible.".into());
                }
                let header = BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: N as i32,
                    biHeight: -(N as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..std::mem::zeroed()
                };
                let info = BITMAPINFO {
                    bmiHeader: header,
                    bmiColors: [RGBQUAD::default()],
                };
                let mut bits = ptr::null_mut();
                let bitmap =
                    CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);
                if bitmap.is_null() || bits.is_null() {
                    if !bitmap.is_null() {
                        DeleteObject(bitmap);
                    }
                    DeleteDC(dc);
                    DestroyWindow(hwnd);
                    return Err("Création de la surface transparente impossible.".into());
                }
                let old = SelectObject(dc, bitmap);
                Ok(Self {
                    hwnd,
                    dc,
                    bitmap,
                    old,
                    bits: bits.cast(),
                })
            }
        }
        fn present(&self, pixels: &[u8], x: i32, y: i32) -> Result<(), String> {
            if pixels.len() != N * N * 4 {
                return Err("Surface des effets invalide.".into());
            }
            // SAFETY: bits points to our N*N*4 DIB, selected in dc. All structures live through the call.
            unsafe {
                ptr::copy_nonoverlapping(pixels.as_ptr(), self.bits, pixels.len());
                let pos = POINT { x, y };
                let size = SIZE {
                    cx: N as i32,
                    cy: N as i32,
                };
                let src = POINT { x: 0, y: 0 };
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                if UpdateLayeredWindow(
                    self.hwnd,
                    ptr::null_mut(),
                    &pos,
                    &size,
                    self.dc,
                    &src,
                    0,
                    &blend,
                    ULW_ALPHA,
                ) == 0
                {
                    return Err(format!(
                        "Rendu des effets : {}",
                        std::io::Error::last_os_error()
                    ));
                }
                ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
            Ok(())
        }
        fn hide(&self) {
            // SAFETY: hwnd is owned by this thread and valid until Layer::drop.
            unsafe {
                ShowWindow(self.hwnd, SW_HIDE);
            }
        }
    }
    impl Drop for Layer {
        fn drop(&mut self) {
            // SAFETY: restore the selected object before deleting our bitmap/DC/window, once each.
            unsafe {
                SelectObject(self.dc, self.old);
                DeleteObject(self.bitmap);
                DeleteDC(self.dc);
                DestroyWindow(self.hwnd);
            }
        }
    }
    struct Dot {
        p: [f32; 2],
        at: Instant,
        right: bool,
    }
    fn circle(
        out: &mut [u8],
        c: [f32; 2],
        r: f32,
        width: Option<f32>,
        color: [u8; 3],
        opacity: f32,
    ) {
        let x0 = (c[0] - r - 1.0).max(0.0) as usize;
        let y0 = (c[1] - r - 1.0).max(0.0) as usize;
        let x1 = ((c[0] + r + 1.0).max(0.0) as usize).min(N);
        let y1 = ((c[1] + r + 1.0).max(0.0) as usize).min(N);
        for y in y0.min(N)..y1 {
            for x in x0.min(N)..x1 {
                let d = ((x as f32 + 0.5 - c[0]).powi(2) + (y as f32 + 0.5 - c[1]).powi(2)).sqrt();
                let mut cover = (r + 0.5 - d).clamp(0.0, 1.0);
                if let Some(w) = width {
                    cover *= (d - r + w + 0.5).clamp(0.0, 1.0);
                }
                let a = (cover * opacity).clamp(0.0, 1.0);
                if a <= 0.0 {
                    continue;
                }
                let at = (y * N + x) * 4;
                for (j, k) in [2, 1, 0].into_iter().enumerate() {
                    out[at + j] =
                        (color[k] as f32 * a + out[at + j] as f32 * (1.0 - a)).round() as u8;
                }
                out[at + 3] = (a * 255.0 + out[at + 3] as f32 * (1.0 - a)).round() as u8;
            }
        }
    }
    pub(super) fn run(
        config: Arc<Mutex<Option<Settings>>>,
        stop: Arc<AtomicBool>,
        emergency: Arc<AtomicBool>,
    ) -> Result<(), String> {
        let mut layer = None;
        let mut pixels = vec![0u8; N * N * 4];
        let mut trail = VecDeque::<Dot>::new();
        let mut clicks = Vec::<Dot>::new();
        let mut old_buttons = [false; 2];
        let mut last = None::<[f32; 2]>;
        while !stop.load(Ordering::Relaxed) {
            // SAFETY: this pumps messages only for windows owned by the worker thread.
            unsafe {
                let mut msg = std::mem::zeroed();
                while PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            let setting = if emergency.load(Ordering::Relaxed) {
                None
            } else {
                config.lock().ok().and_then(|v| v.clone())
            };
            let Some(s) = setting else {
                if let Some(v) = &layer {
                    Layer::hide(v);
                }
                trail.clear();
                clicks.clear();
                last = None;
                old_buttons = crate::native::input().map_or([false; 2], |v| v.1);
                std::thread::sleep(Duration::from_millis(30));
                continue;
            };
            let Some((p, buttons, _)) = crate::native::input() else {
                std::thread::sleep(Duration::from_millis(16));
                continue;
            };
            if layer.is_none() {
                layer = Some(Layer::new()?);
            }
            let now = Instant::now();
            if s.trail && last.is_none_or(|old| (old[0] - p[0]).abs() + (old[1] - p[1]).abs() > 1.0)
            {
                trail.push_back(Dot {
                    p,
                    at: now,
                    right: false,
                });
            }
            last = Some(p);
            while trail.len() > 160
                || trail
                    .front()
                    .is_some_and(|d| now.duration_since(d.at).as_secs_f32() > s.trail_length)
            {
                trail.pop_front();
            }
            if s.clicks {
                for i in 0..2 {
                    if buttons[i] && !old_buttons[i] {
                        if clicks.len() >= 24 {
                            clicks.remove(0);
                        }
                        clicks.push(Dot {
                            p,
                            at: now,
                            right: i == 1,
                        });
                    }
                }
            }
            old_buttons = buttons;
            clicks.retain(|d| now.duration_since(d.at).as_secs_f32() < 0.8);
            pixels.fill(0);
            let origin = [p[0] - N as f32 / 2.0, p[1] - N as f32 / 2.0];
            if s.trail {
                for d in &trail {
                    let f = (1.0 - now.duration_since(d.at).as_secs_f32() / s.trail_length)
                        .clamp(0.0, 1.0);
                    circle(
                        &mut pixels,
                        [d.p[0] - origin[0], d.p[1] - origin[1]],
                        (2.0 + 5.0 * f) * s.strength,
                        None,
                        s.color,
                        0.45 * f,
                    );
                }
            }
            for d in &clicks {
                let t = now.duration_since(d.at).as_secs_f32() / 0.8;
                let color = if d.right { [103, 232, 249] } else { s.color };
                let c = [d.p[0] - origin[0], d.p[1] - origin[1]];
                let r = (8.0 + 50.0 * t) * s.strength;
                match s.effect {
                    1 => {
                        for i in 0..10 {
                            let angle = i as f32 * std::f32::consts::TAU / 10.0;
                            for j in 0..5 {
                                let r = r * (0.65 + 0.35 * j as f32 / 4.0);
                                circle(
                                    &mut pixels,
                                    [c[0] + angle.cos() * r, c[1] + angle.sin() * r],
                                    1.2,
                                    None,
                                    color,
                                    1.0 - t,
                                );
                            }
                        }
                    }
                    2 => {
                        circle(&mut pixels, c, r, None, color, (1.0 - t) * 0.13);
                        circle(&mut pixels, c, r, Some(2.0), color, 1.0 - t);
                    }
                    _ => {
                        circle(&mut pixels, c, r, Some(2.5), color, 1.0 - t);
                        circle(&mut pixels, c, r * 0.65, Some(1.0), color, (1.0 - t) * 0.5);
                    }
                }
            }
            if let Some(v) = &layer {
                v.present(&pixels, origin[0] as i32, origin[1] as i32)?;
            }
            std::thread::sleep(Duration::from_millis(16));
        }
        Ok(())
    }
}
