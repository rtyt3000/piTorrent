//! User themes: JSON files that override colours, roundness, opacity and backdrop of a base mode.
//!
//! Looked up in `<app dir>/themes/`, either `name.json` or `name/theme.json`.

use crate::settings::{Backdrop, app_dir};
use crate::theme::{Mode, Palette};
use iced::Color;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct Skin {
    pub name: String,
    /// "dark" or "light" to force that mode when the theme is picked; leave empty to keep the user's.
    pub base: String,
    /// Overrides for both modes.
    pub colors: HashMap<String, String>,
    /// Overrides for one mode only, laid over `colors`.
    pub dark: HashMap<String, String>,
    pub light: HashMap<String, String>,
    /// Multiplier for every corner radius.
    pub radius: Option<f32>,
    pub opacity: Option<f32>,
    /// UI font family; applies after a restart.
    pub font: Option<String>,
    pub mono_font: Option<String>,
    /// "none", "mica" or "acrylic"
    pub backdrop: Option<String>,
}

impl Skin {
    pub fn mode(&self) -> Option<Mode> {
        match self.base.to_ascii_lowercase().as_str() {
            "light" => Some(Mode::Light),
            "dark" => Some(Mode::Dark),
            _ => None,
        }
    }

    pub fn backdrop(&self) -> Option<Backdrop> {
        Some(match self.backdrop.as_deref()?.to_ascii_lowercase().as_str() {
            "mica" => Backdrop::Mica,
            "acrylic" => Backdrop::Acrylic,
            _ => Backdrop::None,
        })
    }

    pub fn paint(&self, mut p: Palette, mode: Mode) -> Palette {
        let own = if mode == Mode::Light { &self.light } else { &self.dark };
        for (key, value) in self.colors.iter().chain(own) {
            if let Some(c) = parse_color(value) {
                p.set(key, c);
            }
        }
        p
    }
}

impl std::fmt::Display for Skin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

impl PartialEq for Skin {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

/// `#RRGGBB` or `#RRGGBBAA`
fn parse_color(s: &str) -> Option<Color> {
    let h = s.trim().strip_prefix('#')?;
    let n = u32::from_str_radix(h, 16).ok()?;
    match h.len() {
        6 => Some(Color::from_rgb8((n >> 16) as u8, (n >> 8) as u8, n as u8)),
        8 => Some(Color::from_rgba8((n >> 24) as u8, (n >> 16) as u8, (n >> 8) as u8, (n & 0xFF) as f32 / 255.0)),
        _ => None,
    }
}

pub const DEFAULT: &str = "piTorrent";

/// The built-in themes followed by the ones found on disk.
pub fn load_all() -> Vec<Skin> {
    let plain = Skin {
        name: DEFAULT.into(),
        radius: Some(1.0),
        opacity: Some(1.0),
        backdrop: Some("none".into()),
        ..Default::default()
    };
    let mut all = vec![plain];
    all.extend(serde_json::from_str(include_str!("../assets/themes/fluent.json")).into_iter());
    if let Ok(dir) = std::fs::read_dir(app_dir().join("themes")) {
        let mut found: Vec<Skin> = dir
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let file = if path.is_dir() { path.join("theme.json") } else { path.clone() };
                let mut skin = read(&file)?;
                if skin.name.is_empty() {
                    skin.name = path.file_stem()?.to_string_lossy().into_owned();
                }
                Some(skin)
            })
            .collect();
        found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        all.extend(found);
    }
    all
}

fn read(path: &Path) -> Option<Skin> {
    if path.extension()? != "json" {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}
