//! Per-plugin settings: plain values in `plugins.json`, passwords and cookies in the OS
//! credential store (Windows Credential Manager, macOS Keychain, Secret Service).

use super::{PluginMeta, Values};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::HashMap;
use std::path::PathBuf;
#[cfg(test)]
use std::sync::Mutex;

pub trait Secrets: Send + Sync {
    fn get(&self, plugin: &str, key: &str) -> Option<String>;
    /// `None` deletes the secret.
    fn set(&self, plugin: &str, key: &str, value: Option<&str>) -> Result<(), String>;
}

pub struct KeyringSecrets;

impl KeyringSecrets {
    fn entry(plugin: &str, key: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new("piTorrent", &format!("plugin:{plugin}:{key}")).map_err(|e| e.to_string())
    }
}

impl Secrets for KeyringSecrets {
    fn get(&self, plugin: &str, key: &str) -> Option<String> {
        Self::entry(plugin, key).ok()?.get_password().ok()
    }

    fn set(&self, plugin: &str, key: &str, value: Option<&str>) -> Result<(), String> {
        let entry = Self::entry(plugin, key)?;
        match value {
            Some(v) if !v.is_empty() => entry.set_password(v).map_err(|e| e.to_string()),
            _ => match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(e.to_string()),
            },
        }
    }
}

/// For tests: secrets live in memory.
#[cfg(test)]
#[derive(Default)]
pub struct MemorySecrets(Mutex<HashMap<(String, String), String>>);

#[cfg(test)]
impl Secrets for MemorySecrets {
    fn get(&self, plugin: &str, key: &str) -> Option<String> {
        self.0.lock().unwrap().get(&(plugin.to_string(), key.to_string())).cloned()
    }

    fn set(&self, plugin: &str, key: &str, value: Option<&str>) -> Result<(), String> {
        let k = (plugin.to_string(), key.to_string());
        match value {
            Some(v) if !v.is_empty() => self.0.lock().unwrap().insert(k, v.to_string()),
            _ => self.0.lock().unwrap().remove(&k),
        };
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Default)]
struct StoreFile {
    enabled: BTreeMap<String, bool>,
    values: BTreeMap<String, Values>,
}

pub struct Store {
    file: StoreFile,
    path: Option<PathBuf>,
    secrets: Box<dyn Secrets>,
}

impl Store {
    pub fn load(path: Option<PathBuf>, secrets: Box<dyn Secrets>) -> Self {
        let file = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Store { file, path, secrets }
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.file) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Plugins that need no settings are on by default; the others wait until configured.
    pub fn is_enabled(&self, meta: &PluginMeta) -> bool {
        self.file
            .enabled
            .get(&meta.id)
            .copied()
            .unwrap_or_else(|| meta.settings.iter().all(|f| !f.required))
    }

    pub fn set_enabled(&mut self, id: &str, on: bool) {
        self.file.enabled.insert(id.to_string(), on);
        self.save();
    }

    /// Defaults, then stored values, then secrets.
    pub fn values(&self, meta: &PluginMeta) -> Values {
        let stored = self.file.values.get(&meta.id);
        meta.settings
            .iter()
            .filter_map(|f| {
                let value = if f.is_secret() {
                    self.secrets.get(&meta.id, &f.key)
                } else {
                    stored.and_then(|s| s.get(&f.key).cloned())
                };
                value.or_else(|| f.default.clone()).map(|v| (f.key.clone(), v))
            })
            .collect()
    }

    pub fn set_value(&mut self, meta: &PluginMeta, key: &str, value: &str) -> Result<(), String> {
        let field = meta.settings.iter().find(|f| f.key == key).ok_or("unknown setting")?;
        if field.is_secret() {
            return self.secrets.set(&meta.id, key, Some(value));
        }
        self.file.values.entry(meta.id.clone()).or_default().insert(key.to_string(), value.to_string());
        self.save();
        Ok(())
    }

    /// Forgets everything stored for a plugin (when it is removed).
    pub fn forget(&mut self, meta: &PluginMeta) {
        for f in meta.settings.iter().filter(|f| f.is_secret()) {
            let _ = self.secrets.set(&meta.id, &f.key, None);
        }
        self.file.values.remove(&meta.id);
        self.file.enabled.remove(&meta.id);
        self.save();
    }

    /// Titles of required settings that have no value yet.
    pub fn missing_required(&self, meta: &PluginMeta) -> Vec<String> {
        let values = self.values(meta);
        meta.settings
            .iter()
            .filter(|f| f.required && values.get(&f.key).is_none_or(|v| v.trim().is_empty()))
            .map(|f| f.title())
            .collect()
    }
}
