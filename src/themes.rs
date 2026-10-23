//! User themes: JSON files that override colours, roundness, opacity and backdrop of a base mode.
//!
//! Looked up in `<app dir>/themes/`, either `name.json` or `name/theme.json`.

use crate::settings::{Backdrop, app_dir};
use crate::theme::{Mode, Palette};
use iced::Color;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

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

/// The default look, shipped as a theme like any other.
pub fn stock() -> &'static Skin {
    static STOCK: OnceLock<Skin> = OnceLock::new();
    STOCK.get_or_init(|| serde_json::from_str(include_str!("../assets/themes/pitorrent.json")).expect("stock theme"))
}

/// The built-in themes followed by the ones found on disk.
pub fn load_all() -> Vec<Skin> {
    let mut all = vec![stock().clone()];
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

const MAX_FILES: usize = 200;
const MAX_FILE: u64 = 32 << 20;
const MAX_TOTAL: u64 = 64 << 20;
const ALLOWED: [&str; 9] = ["json", "png", "jpg", "jpeg", "webp", "ttf", "otf", "txt", "md"];

/// Unpacks a theme archive into the themes folder and returns the theme's name.
/// The archive holds `theme.json` at its root or inside one top-level folder.
pub fn import_zip(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    if zip.len() > MAX_FILES {
        return Err("too many files".into());
    }
    let mut entries = Vec::new();
    for i in 0..zip.len() {
        let f = zip.by_index(i).map_err(|e| e.to_string())?;
        // `enclosed_name` refuses absolute paths and `..`, so nothing escapes the theme folder.
        if let (false, Some(name)) = (f.is_dir(), f.enclosed_name()) {
            entries.push((i, name, f.size()));
        }
    }
    let manifest = entries
        .iter()
        .filter(|(_, n, _)| n.file_name().is_some_and(|f| f == "theme.json") && n.components().count() <= 2)
        .min_by_key(|(_, n, _)| n.components().count())
        .ok_or("no theme.json")?;
    let root = manifest.1.parent().map(Path::to_path_buf).unwrap_or_default();
    let skin: Skin = {
        let mut text = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(zip.by_index(manifest.0).map_err(|e| e.to_string())?, MAX_FILE), &mut text)
            .map_err(|e| e.to_string())?;
        serde_json::from_slice(&text).map_err(|e| e.to_string())?
    };
    let fallback = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = if skin.name.is_empty() { fallback } else { skin.name.clone() };
    if name == DEFAULT {
        return Err("reserved name".into());
    }
    let folder: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') { c } else { '_' })
        .collect();
    let dest = app_dir().join("themes").join(folder.trim());
    if folder.trim().is_empty() {
        return Err("empty name".into());
    }
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let mut total = 0;
    for (i, name, size) in entries {
        let Ok(rel) = name.strip_prefix(&root) else { continue };
        let allowed = rel.extension().and_then(|e| e.to_str()).is_some_and(|e| ALLOWED.contains(&e.to_ascii_lowercase().as_str()));
        if !allowed {
            continue;
        }
        total += size;
        if size > MAX_FILE || total > MAX_TOTAL {
            let _ = std::fs::remove_dir_all(&dest);
            return Err("too big".into());
        }
        let target = dest.join(rel);
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&target).map_err(|e| e.to_string())?;
        let f = zip.by_index(i).map_err(|e| e.to_string())?;
        std::io::copy(&mut std::io::Read::take(f, MAX_FILE), &mut out).map_err(|e| e.to_string())?;
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_theme_defines_every_colour_in_both_modes() {
        let skin = stock();
        for mode in [Mode::Dark, Mode::Light] {
            for key in [
                "bg", "isl", "ibd", "elev", "line", "bd", "hov", "act", "sel", "txt", "txt2", "mute", "faint", "acc",
                "acc_ink", "acc_hov", "dl", "seed", "pause", "err", "track", "input", "off",
            ] {
                let own = if mode == Mode::Light { &skin.light } else { &skin.dark };
                assert!(own.contains_key(key) || skin.colors.contains_key(key), "{key} missing in {mode:?}");
                assert!(parse_color(own.get(key).or(skin.colors.get(key)).unwrap()).is_some(), "{key} unreadable");
            }
        }
    }

    #[test]
    fn colours_parse_with_and_without_alpha() {
        assert_eq!(parse_color("#FF0000"), Some(Color::from_rgb8(255, 0, 0)));
        assert_eq!(parse_color("#00000080").map(|c| (c.a * 255.0).round() as u8), Some(128));
        assert_eq!(parse_color("red"), None);
    }
}
