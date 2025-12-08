#![allow(dead_code)]

use std::time::Duration;

/// Grace window for the native Settings viewport to present on Windows before
/// falling back to the in-app dialog.
pub fn fallback_grace_duration() -> Duration {
    // winit occasionally delays the first redraw for new viewports; give the OS a
    // bit more time than the non-Windows default to avoid false fallbacks.
    Duration::from_millis(1200)
}

/// Timeout for close requests before giving up on the native window.
pub fn close_watchdog_duration() -> Duration {
    // Windows often ignores WM_CLOSE until the non-client area is interacted
    // with; don't wait too long before forcing a teardown/fallback.
    Duration::from_millis(400)
}

/// Attempt to close a window by its title using Win32 APIs. Used as a last
/// resort when the egui/eframe close command is ignored.
pub fn force_close_settings_window(title: &str) -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};

    let mut wide: Vec<u16> = OsStr::new(title).encode_wide().collect();
    wide.push(0);
    let hwnd = unsafe { FindWindowW(std::ptr::null(), wide.as_ptr()) };
    if hwnd == std::ptr::null_mut() {
        return false;
    }
    let ok_post = unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) } != 0;
    ok_post
}

/// Initial retry budget (frames) for scheduling the native Settings viewport.
pub fn retry_budget_ticks() -> u8 {
    12
}
