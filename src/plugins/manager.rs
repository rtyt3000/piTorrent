use super::host::{self, Env, LogFn};
use super::http::{HostPolicy, PluginHttp, Request};
use super::store::{KeyringSecrets, Store};
use super::{PluginError, PluginMeta, Resolved, SearchOpts, SearchResult, Values};
use base64::Engine as _;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const BUNDLED: &[(&str, &str)] = &[
    ("torznab.js", include_str!("../../assets/plugins/torznab.js")),
    ("internet-archive.js", include_str!("../../assets/plugins/internet-archive.js")),
    ("piratebay.js", include_str!("../../assets/plugins/piratebay.js")),
    ("torrents-csv.js", include_str!("../../assets/plugins/torrents-csv.js")),
    ("limetorrents.js", include_str!("../../assets/plugins/limetorrents.js")),
    ("bitsearch.js", include_str!("../../assets/plugins/bitsearch.js")),
    ("nyaa.js", include_str!("../../assets/plugins/nyaa.js")),
    ("rutor.js", include_str!("../../assets/plugins/rutor.js")),
    ("example.js", include_str!("../../assets/plugins/example.js")),
];

/// Icons of the bundled plugins, by plugin id (the favicons of the services they search).
const BUNDLED_ICONS: &[(&str, &[u8])] = &[
    ("internet-archive", include_bytes!("../../assets/plugins/icons/internet-archive.png")),
    ("piratebay", include_bytes!("../../assets/plugins/icons/piratebay.png")),
    ("limetorrents", include_bytes!("../../assets/plugins/icons/limetorrents.png")),
    ("bitsearch", include_bytes!("../../assets/plugins/icons/bitsearch.png")),
    ("nyaa", include_bytes!("../../assets/plugins/icons/nyaa.png")),
    ("rutor", include_bytes!("../../assets/plugins/icons/rutor.png")),
];

const MAX_ICON_BYTES: usize = 64 << 10;

/// The plugin's own icon (a PNG data URL) or, for a bundled plugin, the one shipped with the app.
fn load_icon(meta: &PluginMeta, bundled: bool) -> Option<iced::widget::image::Handle> {
    let own = meta.icon.as_deref().and_then(|u| u.strip_prefix("data:image/png;base64,")).and_then(|b64| {
        base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok().filter(|b| b.len() <= MAX_ICON_BYTES && b.starts_with(&[0x89, b'P', b'N', b'G']))
    });
    let bytes = own.or_else(|| {
        bundled.then(|| BUNDLED_ICONS.iter().find(|(id, _)| *id == meta.id).map(|(_, b)| b.to_vec())).flatten()
    })?;
    Some(iced::widget::image::Handle::from_bytes(bytes))
}

const MAX_RESULTS: usize = 200;
const SEARCH_TIMEOUT: Duration = Duration::from_secs(45);

/// A loaded plugin. Calls take the settings and a cancel flag, so they can run on any thread.
pub struct Plugin {
    pub meta: PluginMeta,
    pub path: Option<PathBuf>,
    pub bundled: bool,
    pub icon: Option<iced::widget::image::Handle>,
    source: Arc<str>,
    http: PluginHttp,
    state: Arc<Mutex<Map<String, Value>>>,
}

impl Plugin {
    pub fn from_source(source: &str, path: Option<PathBuf>, bundled: bool) -> Result<Plugin, PluginError> {
        let meta = host::read_meta(source)?;
        Ok(Plugin {
            icon: load_icon(&meta, bundled),
            http: PluginHttp::new(&format!("piTorrent/{} (plugin {})", env!("CARGO_PKG_VERSION"), meta.id)),
            meta,
            path,
            bundled,
            source: source.into(),
            state: Arc::new(Mutex::new(Map::new())),
        })
    }

    fn env(&self, settings: Values, cancel: Arc<AtomicBool>, timeout: Duration) -> Env {
        let id = self.meta.id.clone();
        let log: LogFn = Arc::new(move |level, msg| eprintln!("[plugin {id}] {level}: {msg}"));
        Env {
            policy: HostPolicy::new(&self.meta.hosts, &settings),
            http: self.http.clone(),
            settings,
            state: self.state.clone(),
            cancel,
            timeout,
            log,
        }
    }

    pub fn search(
        &self,
        settings: Values,
        query: &str,
        category: Option<String>,
        cancel: Arc<AtomicBool>,
    ) -> Result<Vec<SearchResult>, PluginError> {
        let opts = SearchOpts { category, limit: MAX_RESULTS };
        let env = self.env(settings, cancel, SEARCH_TIMEOUT);
        let out = host::call(&self.source, &env, "search", json!([query, opts]))?;
        let Value::Array(items) = out else {
            return Err("search() must return an array".into());
        };
        Ok(items
            .iter()
            .filter_map(parse_result)
            .take(MAX_RESULTS)
            .map(|mut r| {
                r.plugin_id = self.meta.id.clone();
                r.plugin_name = self.meta.name.clone();
                r
            })
            .collect())
    }

    /// Asks the plugin for a real link when a result only has a details page.
    pub fn resolve(&self, settings: Values, result: &SearchResult, cancel: Arc<AtomicBool>) -> Result<Resolved, PluginError> {
        if !self.meta.has_resolve {
            return Err("this plugin cannot resolve links".into());
        }
        let env = self.env(settings, cancel, Duration::from_secs(30));
        let arg = json!({ "title": result.title, "magnet": result.magnet, "torrentUrl": result.torrent_url, "pageUrl": result.page_url });
        let out = host::call(&self.source, &env, "resolve", json!([arg]))?;
        let resolved: Resolved = serde_json::from_value(out).map_err(|e| PluginError(format!("bad resolve() result: {e}")))?;
        Ok(Resolved {
            magnet: resolved.magnet.filter(|m| m.starts_with("magnet:")),
            torrent_url: resolved.torrent_url.filter(|u| is_http(u)),
            torrent_base64: resolved.torrent_base64,
        })
    }

    /// Downloads a .torrent through the plugin's own session (cookies from a previous login apply).
    pub fn fetch_torrent(&self, settings: Values, url: &str, cancel: Arc<AtomicBool>) -> Result<Vec<u8>, PluginError> {
        let env = self.env(settings, cancel.clone(), Duration::from_secs(30));
        let req = Request { url: url.to_string(), binary: true, ..Default::default() };
        let resp = env.http.request(&env.policy, &cancel, req)?;
        if !resp.ok {
            return Err(format!("the site answered {}", resp.status).into());
        }
        let b64 = resp.base64.unwrap_or_default();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| PluginError(e.to_string()))?;
        // A login page or an error page instead of the file is a common failure.
        if bytes.first() != Some(&b'd') {
            return Err("the site did not return a .torrent file (check the login settings)".into());
        }
        Ok(bytes)
    }

    /// Runs the plugin's optional connection check; the message it returns is shown to the user.
    pub fn check(&self, settings: Values, cancel: Arc<AtomicBool>) -> Result<Option<String>, PluginError> {
        if !self.meta.has_check {
            return Ok(None);
        }
        let env = self.env(settings, cancel, Duration::from_secs(30));
        let out = host::call(&self.source, &env, "check", json!([]))?;
        Ok(out.as_str().map(String::from))
    }
}

fn is_http(u: &str) -> bool {
    reqwest::Url::parse(u).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().replace(',', ".").parse().ok(),
        _ => None,
    }
}

fn text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Reads one result leniently: plugins are hand-written, so numbers may arrive as strings.
fn parse_result(v: &Value) -> Option<SearchResult> {
    let obj = v.as_object()?;
    let get = |k: &str| obj.get(k);
    let title = get("title").and_then(text)?;
    let magnet = get("magnet").and_then(text).filter(|m| m.to_lowercase().starts_with("magnet:"));
    let torrent_url = get("torrentUrl").and_then(text).filter(|u| is_http(u));
    let page_url = get("pageUrl").and_then(text).filter(|u| is_http(u));
    if magnet.is_none() && torrent_url.is_none() && page_url.is_none() {
        return None;
    }
    let nat = |k: &str| get(k).and_then(number).filter(|n| *n >= 0.0);
    Some(SearchResult {
        title,
        size: nat("size").map(|n| n as u64),
        seeders: nat("seeders").map(|n| n as u32),
        leechers: nat("leechers").map(|n| n as u32),
        date: get("date").and_then(text),
        magnet,
        torrent_url,
        page_url,
        category: get("category").and_then(text),
        ..Default::default()
    })
}

pub struct Manager {
    pub plugins: Vec<Arc<Plugin>>,
    pub store: Store,
    pub dir: PathBuf,
    /// Files that could not be loaded, with the reason.
    pub failed: Vec<(String, String)>,
}

impl Manager {
    pub fn load(dir: PathBuf, store: Store) -> Self {
        let mut m = Manager { plugins: Vec::new(), store, dir, failed: Vec::new() };
        for (name, src) in BUNDLED {
            match Plugin::from_source(src, None, true) {
                Ok(p) => m.plugins.push(Arc::new(p)),
                Err(e) => m.failed.push((name.to_string(), e.0)),
            }
        }
        let mut files: Vec<PathBuf> = std::fs::read_dir(&m.dir)
            .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "js")).collect())
            .unwrap_or_default();
        files.sort();
        for path in files {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            match std::fs::read_to_string(&path).map_err(|e| PluginError(e.to_string())).and_then(|s| Plugin::from_source(&s, Some(path.clone()), false)) {
                Ok(p) if m.plugins.iter().any(|x| x.meta.id == p.meta.id) => {
                    m.failed.push((name, format!("a plugin with the id {:?} is already loaded", p.meta.id)))
                }
                Ok(p) => m.plugins.push(Arc::new(p)),
                Err(e) => m.failed.push((name, e.0)),
            }
        }
        m
    }

    pub fn default_store(path: PathBuf) -> Store {
        Store::load(Some(path), Box::new(KeyringSecrets))
    }

    pub fn get(&self, id: &str) -> Option<Arc<Plugin>> {
        self.plugins.iter().find(|p| p.meta.id == id).cloned()
    }

    pub fn settings_of(&self, plugin: &Plugin) -> Values {
        self.store.values(&plugin.meta)
    }

    /// Plugins that are switched on and have everything they need.
    pub fn searchable(&self) -> Vec<Arc<Plugin>> {
        self.plugins
            .iter()
            .filter(|p| self.store.is_enabled(&p.meta) && self.store.missing_required(&p.meta).is_empty())
            .cloned()
            .collect()
    }

    /// Copies a plugin file into the plugins folder after checking that it loads.
    pub fn install(&mut self, from: &Path) -> Result<String, PluginError> {
        let source = std::fs::read_to_string(from).map_err(|e| PluginError(e.to_string()))?;
        let meta = host::read_meta(&source)?;
        if self.plugins.iter().any(|p| p.meta.id == meta.id) {
            return Err(format!("a plugin with the id {:?} is already installed", meta.id).into());
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| PluginError(e.to_string()))?;
        let target = self.dir.join(format!("{}.js", meta.id));
        std::fs::write(&target, &source).map_err(|e| PluginError(e.to_string()))?;
        self.plugins.push(Arc::new(Plugin::from_source(&source, Some(target), false)?));
        Ok(meta.id)
    }

    pub fn remove(&mut self, id: &str) {
        let Some(i) = self.plugins.iter().position(|p| p.meta.id == id && !p.bundled) else {
            return;
        };
        let plugin = self.plugins.remove(i);
        if let Some(path) = &plugin.path {
            let _ = std::fs::remove_file(path);
        }
        self.store.forget(&plugin.meta);
    }
}
