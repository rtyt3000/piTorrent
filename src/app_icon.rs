//! The piTorrent logo, from `assets/app_icon_64.rgba` (made by `scripts/make_icons.py` out of
//! `assets/img.png`): raw 64x64 RGBA with rounded corners.

use iced::widget::image::Handle;
use std::sync::OnceLock;

pub const SIZE: u32 = 64;

const PIXELS: &[u8] = include_bytes!("../assets/app_icon_64.rgba");

pub fn rgba() -> Vec<u8> {
    PIXELS.to_vec()
}

/// For drawing the logo inside the window; created once so it is not re-hashed on every frame.
pub fn handle() -> Handle {
    static HANDLE: OnceLock<Handle> = OnceLock::new();
    HANDLE.get_or_init(|| Handle::from_rgba(SIZE, SIZE, PIXELS.to_vec())).clone()
}

#[cfg(test)]
#[test]
fn icon_data_matches_its_size() {
    assert_eq!(PIXELS.len(), (SIZE * SIZE * 4) as usize);
}
