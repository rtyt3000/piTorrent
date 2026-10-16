//! Runs plugin JavaScript in QuickJS with a small host API (HTTP, HTML, logging, state).

use super::http::{HostPolicy, PluginHttp, Request};
use super::{PluginError, PluginMeta, Values};
use rquickjs::{Context, Ctx, Function, Object, Runtime};
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PRELUDE: &str = include_str!("../../assets/plugins/prelude.js");
const MEMORY_LIMIT: usize = 64 << 20;
const STACK_LIMIT: usize = 1 << 20;

pub type LogFn = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Everything a single plugin call may touch.
#[derive(Clone)]
pub struct Env {
    pub http: PluginHttp,
    pub policy: HostPolicy,
    pub settings: Values,
    /// Small key-value store that survives between calls (for example a login marker).
    pub state: Arc<Mutex<Map<String, Value>>>,
    pub cancel: Arc<AtomicBool>,
    pub timeout: Duration,
    pub log: LogFn,
}

impl Env {
    /// No network, no settings: enough to read a plugin's declaration.
    pub fn bare() -> Self {
        Env {
            http: PluginHttp::new("piTorrent"),
            policy: HostPolicy::default(),
            settings: Values::new(),
            state: Arc::new(Mutex::new(Map::new())),
            cancel: Arc::new(AtomicBool::new(false)),
            timeout: Duration::from_secs(5),
            log: Arc::new(|_, _| {}),
        }
    }
}

fn js_error(ctx: &Ctx<'_>, e: rquickjs::Error) -> String {
    if e.is_exception() {
        let caught = ctx.catch();
        if let Some(ex) = caught.as_exception() {
            let msg = ex.message().unwrap_or_default();
            return if msg.is_empty() { "script error".into() } else { msg };
        }
        return format!("{caught:?}");
    }
    e.to_string()
}

fn run<T>(
    env: &Env,
    source: &str,
    f: impl for<'js> FnOnce(&Ctx<'js>) -> Result<T, String>,
) -> Result<T, PluginError> {
    let rt = Runtime::new().map_err(|e| PluginError(e.to_string()))?;
    rt.set_memory_limit(MEMORY_LIMIT);
    rt.set_max_stack_size(STACK_LIMIT);
    let (deadline, cancel) = (Instant::now() + env.timeout, env.cancel.clone());
    rt.set_interrupt_handler(Some(Box::new(move || {
        cancel.load(Ordering::Relaxed) || Instant::now() > deadline
    })));
    let context = Context::full(&rt).map_err(|e| PluginError(e.to_string()))?;

    context
        .with(|ctx| {
            install_natives(&ctx, env).map_err(|e| js_error(&ctx, e))?;
            ctx.eval::<(), _>(PRELUDE).map_err(|e| format!("prelude: {}", js_error(&ctx, e)))?;
            ctx.eval::<(), _>(source).map_err(|e| js_error(&ctx, e))?;
            f(&ctx)
        })
        .map_err(|msg| {
            if msg.contains("interrupted") {
                PluginError(if env.cancel.load(Ordering::Relaxed) { "cancelled".into() } else { "the plugin took too long".into() })
            } else {
                PluginError(msg)
            }
        })
}

fn install_natives(ctx: &Ctx<'_>, env: &Env) -> rquickjs::Result<()> {
    let n = Object::new(ctx.clone())?;

    let (http, policy, cancel) = (env.http.clone(), env.policy.clone(), env.cancel.clone());
    n.set(
        "http",
        Function::new(ctx.clone(), move |req: String| -> String {
            let result = serde_json::from_str::<Request>(&req)
                .map_err(|e| format!("bad request: {e}"))
                .and_then(|r| http.request(&policy, &cancel, r));
            match result {
                Ok(resp) => serde_json::to_string(&resp).unwrap_or_else(|e| json!({ "error": e.to_string() }).to_string()),
                Err(e) => json!({ "error": e }).to_string(),
            }
        }),
    )?;

    // Parsed documents live on the Rust side; scripts only hold small integer handles.
    let docs: Rc<RefCell<Vec<Doc>>> = Rc::new(RefCell::new(Vec::new()));
    let store = docs.clone();
    n.set(
        "parse",
        Function::new(ctx.clone(), move |html: String| -> i32 {
            let mut docs = store.borrow_mut();
            docs.push(Doc { html: scraper::Html::parse_document(&html), nodes: Vec::new() });
            docs.len() as i32 - 1
        }),
    )?;
    n.set(
        "select",
        Function::new(ctx.clone(), move |doc: i32, node: i32, css: String| -> String {
            select(&mut docs.borrow_mut(), doc, node, &css).to_string()
        }),
    )?;

    let log = env.log.clone();
    n.set("log", Function::new(ctx.clone(), move |level: String, msg: String| log(&level, &msg)))?;

    let state = env.state.clone();
    n.set(
        "statePut",
        Function::new(ctx.clone(), move |json: String| {
            if let Ok(Value::Object(map)) = serde_json::from_str(&json) {
                *state.lock().unwrap() = map;
            }
        }),
    )?;

    ctx.globals().set("__native", n)
}

struct Doc {
    html: scraper::Html,
    /// Elements handed out to the script, by handle.
    nodes: Vec<ego_tree::NodeId>,
}

/// CSS selection below `node` (or in the whole document for -1). Returns
/// `[{id, t: text, h: outer html, a: {attributes}}]` where `id` is a handle for further selects.
fn select(docs: &mut [Doc], doc: i32, node: i32, css: &str) -> Value {
    let selector = match scraper::Selector::parse(css) {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("bad selector {css:?}: {e}") }),
    };
    let Some(entry) = usize::try_from(doc).ok().and_then(|i| docs.get_mut(i)) else {
        return json!({ "error": "unknown document" });
    };
    let found: Vec<ego_tree::NodeId> = {
        let root = if node < 0 {
            Some(entry.html.root_element())
        } else {
            usize::try_from(node)
                .ok()
                .and_then(|i| entry.nodes.get(i))
                .and_then(|id| entry.html.tree.get(*id))
                .and_then(scraper::ElementRef::wrap)
        };
        let Some(root) = root else {
            return json!({ "error": "unknown element" });
        };
        root.select(&selector).map(|el| el.id()).collect()
    };
    let items: Vec<Value> = found
        .into_iter()
        .filter_map(|id| {
            let el = scraper::ElementRef::wrap(entry.html.tree.get(id)?)?;
            let text = el.text().collect::<String>();
            let attrs: Map<String, Value> =
                el.value().attrs().map(|(k, v)| (k.to_string(), Value::String(v.to_string()))).collect();
            let handle = entry.nodes.len();
            entry.nodes.push(id);
            Some(json!({
                "id": handle,
                "t": text.split_whitespace().collect::<Vec<_>>().join(" "),
                "h": el.html(),
                "a": attrs,
            }))
        })
        .collect();
    Value::Array(items)
}

/// Reads the plugin's declaration. The script runs without network access or settings.
pub fn read_meta(source: &str) -> Result<PluginMeta, PluginError> {
    let json = run(&Env::bare(), source, |ctx| {
        let f: Function = ctx.globals().get("__meta").map_err(|e| js_error(ctx, e))?;
        f.call::<_, String>(()).map_err(|e| js_error(ctx, e))
    })?;
    let meta: PluginMeta = serde_json::from_str(&json).map_err(|e| PluginError(format!("invalid plugin declaration: {e}")))?;
    validate(&meta)?;
    Ok(meta)
}

fn validate(meta: &PluginMeta) -> Result<(), PluginError> {
    let id_ok = !meta.id.is_empty()
        && meta.id.len() <= 40
        && meta.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if !id_ok {
        return Err("the plugin id must be 1-40 characters: a-z, 0-9, - and _".into());
    }
    if meta.name.trim().is_empty() {
        return Err("the plugin has no name".into());
    }
    let mut keys = std::collections::HashSet::new();
    for f in &meta.settings {
        if f.key.is_empty() || !keys.insert(&f.key) {
            return Err(format!("setting {:?} is empty or duplicated", f.key).into());
        }
    }
    Ok(())
}

/// Calls one of the plugin's functions (`search`, `resolve`, `check`).
pub fn call(source: &str, env: &Env, method: &str, args: Value) -> Result<Value, PluginError> {
    let state = Value::Object(env.state.lock().unwrap().clone());
    let ctx_json = json!({ "settings": env.settings, "state": state }).to_string();
    let args_json = Value::Array(match args {
        Value::Array(a) => a,
        other => vec![other],
    })
    .to_string();
    let out = run(env, source, |ctx| {
        let f: Function = ctx.globals().get("__call").map_err(|e| js_error(ctx, e))?;
        f.call::<_, String>((method, ctx_json, args_json)).map_err(|e| js_error(ctx, e))
    })?;
    serde_json::from_str(&out).map_err(|e| PluginError(format!("the plugin returned something unreadable: {e}")))
}
