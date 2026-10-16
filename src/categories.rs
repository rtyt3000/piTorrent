use crate::settings::app_dir;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Category {
    pub name: String,
    /// Download folder for the category; empty means the default one.
    pub path: String,
}

/// Categories are stored by info hash, so they survive restarts and torrent moves.
#[derive(Clone, Serialize, Deserialize, Default, Debug)]
pub struct Categories {
    pub list: Vec<Category>,
    pub assigned: HashMap<String, String>,
}

fn file() -> PathBuf {
    app_dir().join("categories.json")
}

impl Categories {
    pub fn load() -> Self {
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

    /// Returns false for an empty or already used name.
    pub fn add(&mut self, name: &str, path: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || self.list.iter().any(|c| c.name.eq_ignore_ascii_case(name)) {
            return false;
        }
        self.list.push(Category { name: name.to_string(), path: path.trim().to_string() });
        true
    }

    pub fn remove(&mut self, name: &str) {
        self.list.retain(|c| c.name != name);
        self.assigned.retain(|_, c| c != name);
    }

    pub fn set(&mut self, hash: &str, category: Option<&str>) {
        match category {
            Some(c) if self.list.iter().any(|x| x.name == c) => {
                self.assigned.insert(hash.to_string(), c.to_string());
            }
            _ => {
                self.assigned.remove(hash);
            }
        }
    }

    pub fn of(&self, hash: &str) -> Option<&str> {
        self.assigned.get(hash).map(String::as_str)
    }

    pub fn path_of(&self, name: &str) -> Option<&str> {
        self.list
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.path.as_str())
            .filter(|p| !p.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_rejects_empty_and_duplicate_names() {
        let mut c = Categories::default();
        assert!(c.add("Movies", "D:/Movies"));
        assert!(!c.add("movies", ""));
        assert!(!c.add("  ", ""));
        assert_eq!(c.path_of("Movies"), Some("D:/Movies"));
    }

    #[test]
    fn empty_path_means_default() {
        let mut c = Categories::default();
        c.add("Misc", "");
        assert_eq!(c.path_of("Misc"), None);
    }

    #[test]
    fn assignment_follows_existing_categories_only() {
        let mut c = Categories::default();
        c.add("Linux", "");
        c.set("abc", Some("Linux"));
        assert_eq!(c.of("abc"), Some("Linux"));
        c.set("def", Some("Nope"));
        assert_eq!(c.of("def"), None);
        c.set("abc", None);
        assert_eq!(c.of("abc"), None);
    }

    #[test]
    fn removing_a_category_unassigns_torrents() {
        let mut c = Categories::default();
        c.add("Linux", "");
        c.set("abc", Some("Linux"));
        c.remove("Linux");
        assert_eq!(c.of("abc"), None);
        assert!(c.list.is_empty());
    }
}
