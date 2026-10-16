//! OS-specific window tweaks.

#[cfg(windows)]
mod imp {
    use iced::window::Window;
    use raw_window_handle::RawWindowHandle;
    use std::ffi::c_void;

    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(hwnd: isize, attribute: u32, value: *const c_void, size: u32) -> i32;
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
}

pub use imp::round_corners;
