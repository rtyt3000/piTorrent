//! OS-specific window tweaks.

#[cfg(windows)]
mod imp {
    use crate::settings::Backdrop;
    use iced::window::Window;
    use raw_window_handle::RawWindowHandle;
    use std::ffi::c_void;

    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(hwnd: isize, attribute: u32, value: *const c_void, size: u32) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn SetProcessWorkingSetSize(process: isize, min: usize, max: usize) -> i32;
    }

    /// Asks Windows to move the idle pages of the process out of RAM; they come back on demand.
    /// Used while the window sits in the tray, so the process shows a small memory footprint.
    pub fn trim_memory() {
        // SAFETY: both calls take no pointers; (-1, -1) is the documented way to trim.
        unsafe {
            SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
        }
    }

    const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;

    /// Mica / acrylic behind the window (Windows 11 22H2+; a no-op on older versions).
    pub fn set_backdrop(window: &dyn Window, backdrop: Backdrop) {
        let Ok(handle) = window.window_handle() else {
            return;
        };
        if let RawWindowHandle::Win32(h) = handle.as_raw() {
            let kind: u32 = match backdrop {
                Backdrop::None => 1,
                Backdrop::Mica => 2,
                Backdrop::Acrylic => 3,
            };
            // SAFETY: the HWND belongs to our live window and the value outlives the call.
            unsafe {
                DwmSetWindowAttribute(
                    h.hwnd.get(),
                    DWMWA_SYSTEMBACKDROP_TYPE,
                    &kind as *const u32 as *const c_void,
                    size_of::<u32>() as u32,
                );
            }
        }
    }

    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_ROUND: u32 = 2;

    /// Rounded corners for the borderless window (Windows 11; a no-op on older versions).
    pub fn round_corners(window: &dyn Window) {
        let Ok(handle) = window.window_handle() else {
            return;
        };
        if let RawWindowHandle::Win32(h) = handle.as_raw() {
            let preference = DWMWCP_ROUND;
            // SAFETY: the HWND belongs to our live window and the value outlives the call.
            unsafe {
                DwmSetWindowAttribute(
                    h.hwnd.get(),
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    &preference as *const u32 as *const c_void,
                    size_of::<u32>() as u32,
                );
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn round_corners(_: &dyn iced::window::Window) {}

    pub fn set_backdrop(_: &dyn iced::window::Window, _: crate::settings::Backdrop) {}

    /// Gives freed heap memory back to the OS while the window sits in the tray (glibc only).
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    pub fn trim_memory() {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> i32;
        }
        // SAFETY: takes no pointers and is safe to call at any time.
        unsafe {
            malloc_trim(0);
        }
    }

    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    pub fn trim_memory() {}
}

pub use imp::{round_corners, set_backdrop, trim_memory};
