use crate::i18n::Language;
use crate::theme::Mode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Where the window was when it was last closed (logical pixels).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// System material behind a translucent window (Windows 11 only; ignored elsewhere).
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum Backdrop {
    #[default]
    None,
    Mica,
    Acrylic,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Mode,
    /// Alpha of the window surfaces, 0.3..=1.0
    pub opacity: f32,
    pub backdrop: Backdrop,
    /// Name of the chosen theme from `themes`; none means the stock look
    pub skin: Option<String>,
    pub language: Language,
    pub close_to_tray: bool,
    pub autostart: bool,
    pub notify_complete: bool,
    pub download_dir: String,
    pub start_now: bool,
    pub port: u16,
    pub dht: bool,
    /// 0 means unlimited
    pub max_downloads: u32,
    pub max_seeds: u32,
    pub down_limit: u32,
    pub up_limit: u32,
    pub window: Option<WindowGeometry>,
}

impl Default for Settings {
    fn default() -> Self {
        let dir = dirs::download_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Torrents");
        Self {
            theme: Mode::Dark,
            opacity: 1.0,
            backdrop: Backdrop::None,
            skin: None,
            language: Language::Auto,
            close_to_tray: true,
            autostart: false,
            notify_complete: true,
            download_dir: dir.to_string_lossy().into_owned(),
            start_now: true,
            port: 51413,
            dht: true,
            max_downloads: 3,
            max_seeds: 5,
            down_limit: 0,
            up_limit: 0,
            window: None,
        }
    }
}

fn config_root() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// `PITORRENT_PROFILE` selects a separate data folder, so a second copy can run side by side.
pub fn profile() -> Option<String> {
    std::env::var("PITORRENT_PROFILE").ok().filter(|p| !p.is_empty())
}

pub fn app_dir() -> PathBuf {
    match profile() {
        Some(p) => config_root().join(format!("piTorrent-{p}")),
        None => config_root().join("piTorrent"),
    }
}

/// Keeps settings and torrents from before the app was renamed from Tidepool.
fn migrate_legacy_dir() {
    if profile().is_some() {
        return;
    }
    let (old, new) = (config_root().join("Tidepool"), app_dir());
    if old.exists() && !new.exists() {
        let _ = std::fs::rename(old, new);
    }
}

fn file() -> PathBuf {
    app_dir().join("settings.json")
}

impl Settings {
    pub fn load() -> Self {
        migrate_legacy_dir();
        std::fs::read_to_string(file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(app_dir());
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(file(), s);
        }
    }
}
