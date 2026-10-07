//! Remember which window the user was typing in, and give focus back to it
//! before pasting.
//!
//! - Windows: capture the foreground HWND when the popup opens and call
//!   `SetForegroundWindow` on commit. Allowed because our process owns the
//!   foreground at that moment.
//! - macOS: the app runs as an accessory (no Dock icon); hiding the app
//!   (`AppHandle::hide`) hands focus back to the previously active app.
//! - Linux/X11: hiding the popup returns focus to the previous window in
//!   most window managers.

#[derive(Debug, Clone, Copy)]
pub struct PrevWindow(#[allow(dead_code)] isize);

#[cfg(windows)]
pub fn capture() -> Option<PrevWindow> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.is_invalid()).then(|| PrevWindow(hwnd.0 as isize))
}

#[cfg(windows)]
pub fn restore(prev: Option<PrevWindow>) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
    if let Some(PrevWindow(h)) = prev {
        let ok = unsafe { SetForegroundWindow(HWND(h as *mut core::ffi::c_void)) };
        if !ok.as_bool() {
            tracing::warn!("SetForegroundWindow failed; pasting into whatever has focus");
        }
    }
}

#[cfg(not(windows))]
pub fn capture() -> Option<PrevWindow> {
    None
}

#[cfg(not(windows))]
pub fn restore(_prev: Option<PrevWindow>) {}
