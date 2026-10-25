//! Small audited Win32 boundary. Bitmap data stays alive until GDI has copied it.
use crate::model::{self, Settings};
#[derive(Default)]
pub struct CursorSession {
    pub applied: bool,
}
impl Drop for CursorSession {
    fn drop(&mut self) {
        if self.applied {
            let _ = restore();
        }
    }
}
impl CursorSession {
    pub fn apply_package(
        &mut self,
        package: &crate::zcursor::Package,
        scale: f64,
    ) -> Result<(), String> {
        #[cfg(windows)]
        {
            self.reset()?;
            for sprite in package.roles(scale) {
                if let Err(e) = win::install_sprite(sprite) {
                    let _ = self.reset();
                    return Err(e);
                }
                self.applied = true;
            }
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = (package, scale);
            Err("L'application système est disponible sous Windows.".into())
        }
    }
    pub fn apply(&mut self, s: &Settings) -> Result<(), String> {
        let pixels = model::raster(s)?;
        if self.applied {
            self.reset()?;
        }
        apply_pixels(s, &pixels)?;
        self.applied = true;
        Ok(())
    }
    pub fn reset(&mut self) -> Result<(), String> {
        restore()?;
        self.applied = false;
        Ok(())
    }
}
#[cfg(windows)]
mod win {
    use super::*;
    use std::{mem::size_of, ptr};
    use windows_sys::Win32::{
        Foundation::POINT,
        Graphics::Gdi::*,
        UI::{Input::KeyboardAndMouse::GetAsyncKeyState, WindowsAndMessaging::*},
    };
    fn failure(action: &str) -> String {
        format!("{action} : {}", std::io::Error::last_os_error())
    }
    pub fn load_sprite(sprite: &crate::zcursor::Sprite) -> Result<HCURSOR, String> {
        use std::{io::Write, os::windows::ffi::OsStrExt};
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let ext = if sprite.entry.frame_count > 1 {
            "ani"
        } else {
            "cur"
        };
        let path =
            std::env::temp_dir().join(format!("zendaya-{}-{stamp}.{ext}", std::process::id()));
        let mut created = false;
        let result = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            created = true;
            file.write_all(&sprite.windows_file())
                .map_err(|e| e.to_string())?;
            drop(file);
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            // SAFETY: the path is terminated and valid during the call. The nonshared handle is owned.
            let handle = unsafe { LoadCursorFromFileW(wide.as_ptr()) };
            if handle.is_null() {
                Err(failure("Lecture du curseur ZCursor"))
            } else {
                Ok(handle)
            }
        })();
        if created {
            let _ = std::fs::remove_file(path);
        }
        result
    }
    pub fn install_sprite(sprite: &crate::zcursor::Sprite) -> Result<(), String> {
        let Some(role) = crate::zcursor::windows_role(&sprite.entry.role) else {
            return Ok(());
        };
        let cursor = load_sprite(sprite)?;
        // SAFETY: SetSystemCursor consumes this owned nonshared CUR/ANI handle, including on failure.
        if unsafe { SetSystemCursor(cursor, role) } == 0 {
            Err(failure("Application du thème ZCursor"))
        } else {
            Ok(())
        }
    }
    pub fn make(s: &Settings, pixels: &[u8]) -> Result<HICON, String> {
        let n = s.size as usize;
        if !(24..=96).contains(&n) || pixels.len() != n * n * 4 {
            return Err("Pixels de curseur invalides.".into());
        }
        // SAFETY: descriptors are initialized, bounded, and buffers have n*n*4 bytes.
        // GDI bitmap handles are released after CreateIconIndirect copies their contents.
        unsafe {
            let header = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: n as i32,
                biHeight: -(n as i32),
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
            let color = CreateDIBSection(
                ptr::null_mut(),
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                ptr::null_mut(),
                0,
            );
            if color.is_null() || bits.is_null() {
                if !color.is_null() {
                    DeleteObject(color);
                }
                return Err(failure("Création du bitmap"));
            }
            let mut bgra = Vec::with_capacity(pixels.len());
            for p in pixels.as_chunks::<4>().0 {
                let a = p[3] as u16;
                bgra.extend_from_slice(&[
                    (p[2] as u16 * a / 255) as u8,
                    (p[1] as u16 * a / 255) as u8,
                    (p[0] as u16 * a / 255) as u8,
                    p[3],
                ]);
            }
            ptr::copy_nonoverlapping(bgra.as_ptr(), bits.cast::<u8>(), bgra.len());
            let mask_data = vec![0u8; n.div_ceil(16) * 2 * n];
            let mask = CreateBitmap(n as i32, n as i32, 1, 1, mask_data.as_ptr().cast());
            if mask.is_null() {
                DeleteObject(color);
                return Err(failure("Création du masque"));
            }
            let hotspot = model::hotspot(s);
            let info = ICONINFO {
                fIcon: 0,
                xHotspot: hotspot[0],
                yHotspot: hotspot[1],
                hbmMask: mask,
                hbmColor: color,
            };
            let handle = CreateIconIndirect(&info);
            DeleteObject(mask);
            DeleteObject(color);
            if handle.is_null() {
                Err(failure("Création du curseur"))
            } else {
                Ok(handle)
            }
        }
    }
    pub fn apply(s: &Settings, pixels: &[u8]) -> Result<(), String> {
        let cursor = make(s, pixels)?;
        // SAFETY: CopyIcon produces owned handles. SetSystemCursor consumes each copy;
        // the original is released exactly once. On failure Windows settings are restored.
        unsafe {
            let mut result = Ok(());
            for role in [OCR_NORMAL, OCR_HAND] {
                let copy = CopyIcon(cursor);
                if copy.is_null() {
                    result = Err(failure("Copie du curseur"));
                    break;
                }
                if SetSystemCursor(copy, role) == 0 {
                    result = Err(failure("Application du curseur"));
                    break;
                }
            }
            DestroyIcon(cursor);
            if result.is_err() {
                let _ = reset();
            }
            result
        }
    }
    pub fn reset() -> Result<(), String> {
        // SAFETY: SPI_SETCURSORS takes no pointer and reloads the configured user scheme.
        unsafe {
            if SystemParametersInfoW(SPI_SETCURSORS, 0, ptr::null_mut(), 0) == 0 {
                Err(failure("Restauration Windows"))
            } else {
                Ok(())
            }
        }
    }
    pub fn input() -> Option<([f32; 2], [bool; 2], bool)> {
        // SAFETY: GetCursorPos writes one initialized POINT; key calls take fixed VK codes.
        unsafe {
            let mut p = POINT::default();
            if GetCursorPos(&mut p) == 0 {
                return None;
            }
            let down = |key| GetAsyncKeyState(key) < 0;
            Some((
                [p.x as f32, p.y as f32],
                [down(1), down(2)],
                down(0x11) && down(0x12) && down(0x52),
            ))
        }
    }

    pub fn self_test() -> Result<(), String> {
        for i in 0..model::THEMES.len() {
            let mut s = Settings::default();
            s.preset(i);
            let pixels = model::raster(&s)?;
            let handle = make(&s, &pixels)?;
            // SAFETY: make returns one owned valid handle, never installed in the system.
            unsafe {
                DestroyIcon(handle);
            }
            let file = std::env::temp_dir().join(format!(
                "zendaya-cursor-test-{}-{i}.cur",
                std::process::id()
            ));
            std::fs::write(&file, model::cur_bytes(&s, &pixels)?).map_err(|e| e.to_string())?;
            use std::os::windows::ffi::OsStrExt;
            let path: Vec<u16> = file.as_os_str().encode_wide().chain(Some(0)).collect();
            // SAFETY: path is terminated UTF-16 and stays alive for this call.
            let loaded = unsafe { LoadCursorFromFileW(path.as_ptr()) };
            let error = if loaded.is_null() {
                Some(failure("Relecture du fichier .cur"))
            } else {
                None
            };
            if !loaded.is_null() {
                // SAFETY: LoadCursorFromFileW returns a nonshared cursor owned by us.
                unsafe {
                    DestroyCursor(loaded);
                }
            }
            let _ = std::fs::remove_file(&file);
            if let Some(e) = error {
                return Err(e);
            }
        }
        Ok(())
    }
}
pub fn check_package(package: &crate::zcursor::Package) -> Result<(), String> {
    #[cfg(windows)]
    for sprite in package.cursors.iter().chain(&package.effects) {
        let cursor = win::load_sprite(sprite)?;
        // SAFETY: this owned test handle has never been installed or shared.
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::DestroyCursor(cursor);
        }
    }
    #[cfg(not(windows))]
    let _ = package;
    Ok(())
}
#[cfg(windows)]
pub fn restore() -> Result<(), String> {
    win::reset()
}
#[cfg(windows)]
fn apply_pixels(s: &Settings, p: &[u8]) -> Result<(), String> {
    win::apply(s, p)
}
#[cfg(windows)]
pub fn input() -> Option<([f32; 2], [bool; 2], bool)> {
    win::input()
}
#[cfg(windows)]
pub fn self_test() -> Result<(), String> {
    win::self_test()
}
#[cfg(not(windows))]
pub fn restore() -> Result<(), String> {
    Ok(())
}
#[cfg(not(windows))]
fn apply_pixels(_: &Settings, _: &[u8]) -> Result<(), String> {
    Err("L'application système est disponible sous Windows.".into())
}
#[cfg(not(windows))]
pub fn input() -> Option<([f32; 2], [bool; 2], bool)> {
    None
}
#[cfg(not(windows))]
pub fn self_test() -> Result<(), String> {
    Ok(())
}

/// Recovery keys are sampled independently of repainting, including while minimized.
pub struct Recovery {
    armed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    fired: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Recovery {
    pub fn new() -> Self {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let armed = Arc::new(AtomicBool::new(false));
        let thread_armed = armed.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let fired = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread_fired = fired.clone();
        std::thread::spawn(move || {
            let mut was_down = false;
            #[cfg(windows)]
            let mut registered = false;
            while !thread_stop.load(Ordering::Relaxed) {
                let armed = thread_armed.load(Ordering::Relaxed);
                let down = input().is_some_and(|v| v.2);
                let mut triggered = down && !was_down;
                #[cfg(windows)]
                {
                    use windows_sys::Win32::UI::{
                        Input::KeyboardAndMouse::{
                            MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
                        },
                        WindowsAndMessaging::{PM_REMOVE, PeekMessageW, WM_HOTKEY},
                    };
                    // SAFETY: thread-local hotkey registration and message queue, no window pointer.
                    unsafe {
                        if armed && !registered {
                            registered = RegisterHotKey(
                                std::ptr::null_mut(),
                                1,
                                MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
                                0x52,
                            ) != 0;
                        }
                        if !armed && registered {
                            UnregisterHotKey(std::ptr::null_mut(), 1);
                            registered = false;
                        }
                        if registered {
                            triggered = false;
                            let mut msg = std::mem::zeroed();
                            while PeekMessageW(
                                &mut msg,
                                std::ptr::null_mut(),
                                WM_HOTKEY,
                                WM_HOTKEY,
                                PM_REMOVE,
                            ) != 0
                            {
                                if msg.wParam == 1 {
                                    triggered = true;
                                }
                            }
                        }
                    }
                }
                if armed && triggered && restore().is_ok() {
                    thread_armed.store(false, Ordering::Relaxed);
                    thread_fired.store(true, Ordering::Relaxed);
                }
                was_down = down;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            #[cfg(windows)]
            if registered {
                // SAFETY: release only this worker's thread-local hotkey.
                unsafe {
                    windows_sys::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey(
                        std::ptr::null_mut(),
                        1,
                    );
                }
            }
        });

        Self { stop, fired, armed }
    }
    pub fn signal(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.fired.clone()
    }
    pub fn set_active(&self, active: bool) {
        self.armed
            .store(active, std::sync::atomic::Ordering::Relaxed);
    }
    pub fn take_emergency(&self) -> bool {
        self.fired.swap(false, std::sync::atomic::Ordering::Relaxed)
    }
}
impl Drop for Recovery {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn windows_accepts_every_generated_cursor_and_export() {
        super::self_test().unwrap();
    }
}
