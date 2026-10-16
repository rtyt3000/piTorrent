//! Search plugins: small synchronous JavaScript files run in a sandboxed QuickJS runtime. A plugin
//! declares its settings (API keys, logins, cookies), which hosts it may talk to, and a `search()`
//! function; the host provides HTTP (with a per-plugin cookie jar), HTML parsing and a state store.

mod host;
mod http;
mod manager;
mod store;

pub use manager::{Manager, Plugin};
#[cfg(test)]
pub use store::{KeyringSecrets, MemorySecrets, Store};

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A text that may be given per language: `"plain"` or `{ en: "...", ru: "..." }`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct L10n(BTreeMap<String, String>);

impl L10n {
    /// The text in the current UI language, else English, else whatever there is.
    pub fn get(&self) -> String {
        self.get_in(match crate::i18n::current() {
            crate::i18n::Lang::Ru => "ru",
            crate::i18n::Lang::En => "en",
        })
    }

    pub fn get_in(&self, lang: &str) -> String {
        [lang, "en", "*"]
            .iter()
            .find_map(|k| self.0.get(*k))
            .or_else(|| self.0.values().next())
            .cloned()
            .unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.0.values().all(|v| v.is_empty())
    }
}

impl From<&str> for L10n {
    fn from(s: &str) -> Self {
        L10n(BTreeMap::from([("*".to_string(), s.to_string())]))
    }
}

impl<'de> Deserialize<'de> for L10n {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Plain(String),
            Map(BTreeMap<String, String>),
        }
        Ok(match Raw::deserialize(d)? {
            Raw::Plain(s) => L10n::from(s.as_str()),
            Raw::Map(m) => L10n(m),
        })
    }
}

impl Serialize for L10n {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    #[default]
    Text,
    Password,
    Cookie,
    Url,
    Number,
    Bool,
    Select,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SelectOption {
    pub value: String,
    #[serde(default)]
    pub label: L10n,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SettingField {
    pub key: String,
    #[serde(default)]
    pub label: L10n,
    #[serde(rename = "type", default)]
    pub kind: FieldKind,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub help: Option<L10n>,
    #[serde(default)]
    pub options: Vec<SelectOption>,
}

impl SettingField {
    /// Passwords and cookies are kept in the OS credential store, not in a plain file.
    pub fn is_secret(&self) -> bool {
        matches!(self.kind, FieldKind::Password | FieldKind::Cookie)
    }

    pub fn title(&self) -> String {
        if self.label.is_empty() { self.key.clone() } else { self.label.get() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PluginMeta {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: L10n,
    #[serde(default)]
    pub author: String,
    /// Hosts the plugin may connect to; `$setting:key` takes the host from a URL setting.
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub settings: Vec<SettingField>,
    #[serde(default)]
    pub has_resolve: bool,
    #[serde(default)]
    pub has_check: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub seeders: Option<u32>,
    #[serde(default)]
    pub leechers: Option<u32>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub magnet: Option<String>,
    #[serde(default)]
    pub torrent_url: Option<String>,
    #[serde(default)]
    pub page_url: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    /// Filled in by the host.
    #[serde(skip)]
    pub plugin_id: String,
    #[serde(skip)]
    pub plugin_name: String,
}

/// What `resolve()` returns: the link the result can really be added by.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Resolved {
    #[serde(default)]
    pub magnet: Option<String>,
    #[serde(default)]
    pub torrent_url: Option<String>,
    /// A .torrent the plugin downloaded itself (for sites that need special headers).
    #[serde(default)]
    pub torrent_base64: Option<String>,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct SearchOpts {
    pub category: Option<String>,
    pub limit: usize,
}

pub type Values = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginError(pub String);

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for PluginError {
    fn from(s: String) -> Self {
        PluginError(s)
    }
}

impl From<&str> for PluginError {
    fn from(s: &str) -> Self {
        PluginError(s.to_string())
    }
}

#[cfg(test)]
mod tests;
