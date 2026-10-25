//! A click-through, per-pixel-alpha Win32 layer. Coordinates are physical pixels.
use crate::model::Settings;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
type PackageSlot = Arc<Mutex<Option<Arc<crate::zcursor::Package>>>>;
pub struct DesktopEffects {
    package: PackageSlot,
    config: Arc<Mutex<Option<Settings>>>,
    error: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl DesktopEffects {
    pub fn new(emergency: Arc<AtomicBool>) -> Self {
        let package: PackageSlot = Arc::new(Mutex::new(None));
        let worker_package = package.clone();
        let config = Arc::new(Mutex::new(None));
        let error = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let c = config.clone();
        let e = error.clone();
        let s = stop.clone();
        let worker = std::thread::spawn(move || run(c, e, s, emergency, worker_package));
        Self {
            package,
            config,
            error,
            stop,
            worker: Some(worker),
        }
    }
    pub fn set_package(&self, package: Option<Arc<crate::zcursor::Package>>) {
        if let Ok(mut slot) = self.package.lock() {
            *slot = package;
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
fn run(
    _: Arc<Mutex<Option<Settings>>>,
    _: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
    emergency: Arc<AtomicBool>,
    _: PackageSlot,
) {
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
    package: PackageSlot,
) {
    if let Err(e) = win::run(config, stop, emergency, package)
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
    }
    fn blit(
        out: &mut [u8],
        sprite: &crate::zcursor::Sprite,
        age: u64,
        pos: [f32; 2],
        strength: f32,
    ) {
        let e = &sprite.entry;
        let frame = sprite.frame_at(age);
        let width = e.frame_size.width;
        let left = pos[0].round() as i32 - e.hotspot.x as i32;
        let top = pos[1].round() as i32 - e.hotspot.y as i32;
        for y in 0..e.frame_size.height {
            for x in 0..width {
                let (dx, dy) = (left + x as i32, top + y as i32);
                if dx < 0 || dy < 0 || dx >= N as i32 || dy >= N as i32 {
                    continue;
                }
                let p = sprite.pixels.get_pixel(frame * width + x, y).0;
                let a = (p[3] as f32 / 255.0 * strength).clamp(0.0, 1.0);
                let at = (dy as usize * N + dx as usize) * 4;
                for (j, k) in [2, 1, 0].into_iter().enumerate() {
                    out[at + j] = (p[k] as f32 * a + out[at + j] as f32 * (1.0 - a)).round() as u8;
                }
                out[at + 3] = (a * 255.0 + out[at + 3] as f32 * (1.0 - a)).round() as u8;
            }
        }
    }
    pub(super) fn run(
        config: Arc<Mutex<Option<Settings>>>,
        stop: Arc<AtomicBool>,
        emergency: Arc<AtomicBool>,
        package: PackageSlot,
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
            let package = package.lock().ok().and_then(|p| p.clone());
            let now = Instant::now();
            if s.trail && last.is_none_or(|old| (old[0] - p[0]).abs() + (old[1] - p[1]).abs() > 1.0)
            {
                trail.push_back(Dot { p, at: now });
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
                        clicks.push(Dot { p, at: now });
                    }
                }
            }
            old_buttons = buttons;
            let click_lifetime = package
                .as_ref()
                .and_then(|p| {
                    p.effects
                        .iter()
                        .filter(|s| s.entry.role != "cursorfx-trail")
                        .map(|s| s.duration_ms() as f32 / 1000.0)
                        .reduce(f32::max)
                })
                .unwrap_or(0.8)
                / s.speed;
            let click_lifetime = click_lifetime.min(30.0);
            clicks.retain(|d| now.duration_since(d.at).as_secs_f32() < click_lifetime);
            pixels.fill(0);
            let origin = [p[0] - N as f32 / 2.0, p[1] - N as f32 / 2.0];
            if let Some(package) = &package {
                for sprite in package.effects.iter().take(8) {
                    let dots: Vec<_> = if sprite.entry.role == "cursorfx-trail" {
                        trail.iter().rev().step_by(4).take(40).collect()
                    } else {
                        clicks.iter().collect()
                    };
                    for dot in dots {
                        let age = (now.duration_since(dot.at).as_millis() as f64
                            * f64::from(s.speed)) as u64;
                        if age < sprite.duration_ms() {
                            blit(
                                &mut pixels,
                                sprite,
                                age,
                                [dot.p[0] - origin[0], dot.p[1] - origin[1]],
                                s.strength,
                            );
                        }
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
