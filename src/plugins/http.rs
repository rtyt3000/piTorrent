//! The HTTP side of the plugin sandbox: allowed hosts only, a cookie jar per plugin, redirects
//! checked against the same rules, bounded time and size.

use super::Values;
use base64::Engine as _;
use reqwest::blocking::Client;
use reqwest::cookie::Jar;
use reqwest::header::{CONTENT_TYPE, HeaderName, HeaderValue, LOCATION};
use reqwest::redirect::Policy;
use reqwest::{Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_MAX_BYTES: u64 = 8 << 20;
const MAX_REDIRECTS: usize = 8;

/// Which hosts a plugin may talk to.
#[derive(Clone, Debug, Default)]
pub struct HostPolicy {
    patterns: Vec<String>,
}

impl HostPolicy {
    /// `hosts` come from the plugin; `$setting:key` entries use the host of a URL setting.
    pub fn new(hosts: &[String], settings: &Values) -> Self {
        let mut patterns = Vec::new();
        for h in hosts {
            if let Some(key) = h.strip_prefix("$setting:") {
                let host = settings
                    .get(key)
                    .and_then(|v| Url::parse(v.trim()).ok())
                    .and_then(|u| u.host_str().map(str::to_lowercase));
                patterns.extend(host);
            } else {
                patterns.push(h.trim().to_lowercase());
            }
        }
        Self { patterns }
    }

    pub fn allows(&self, url: &Url) -> bool {
        if !matches!(url.scheme(), "http" | "https") {
            return false;
        }
        let Some(host) = url.host_str().map(str::to_lowercase) else {
            return false;
        };
        self.patterns
            .iter()
            .any(|p| host == *p || host.strip_suffix(p.as_str()).is_some_and(|rest| rest.ends_with('.')))
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Request {
    pub url: String,
    pub method: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub query: BTreeMap<String, Value>,
    pub form: Option<BTreeMap<String, Value>>,
    pub body: Option<String>,
    pub json: Option<Value>,
    /// Charset of the form body and fallback charset of the response, e.g. "windows-1251".
    pub encoding: Option<String>,
    pub timeout_ms: Option<u64>,
    pub max_bytes: Option<u64>,
    pub binary: bool,
}

#[derive(Serialize, Debug)]
pub struct Response {
    pub status: u16,
    pub ok: bool,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub text: String,
    pub base64: Option<String>,
}

/// A plugin's connection state: the cookie jar survives between searches, so a login is reused.
#[derive(Clone)]
pub struct PluginHttp {
    jar: Arc<Jar>,
    // Built on first use, on the (blocking) worker thread, never inside an async context.
    client: Arc<Mutex<Option<Client>>>,
    user_agent: String,
}

impl PluginHttp {
    pub fn new(user_agent: &str) -> Self {
        Self {
            jar: Arc::new(Jar::default()),
            client: Arc::new(Mutex::new(None)),
            user_agent: user_agent.to_string(),
        }
    }

    fn client(&self) -> Result<Client, String> {
        let mut slot = self.client.lock().unwrap();
        if slot.is_none() {
            let client = Client::builder()
                .cookie_provider(self.jar.clone())
                .redirect(Policy::none())
                .user_agent(self.user_agent.clone())
                .build()
                .map_err(|e| e.to_string())?;
            *slot = Some(client);
        }
        Ok(slot.clone().unwrap())
    }

    pub fn request(&self, policy: &HostPolicy, cancel: &AtomicBool, req: Request) -> Result<Response, String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let client = self.client()?;
        let mut url = Url::parse(&req.url).map_err(|e| format!("bad URL {:?}: {e}", req.url))?;
        for (k, v) in &req.query {
            url.query_pairs_mut().append_pair(k, &value_to_string(v));
        }
        let mut method = Method::from_bytes(req.method.as_deref().unwrap_or("GET").to_uppercase().as_bytes())
            .map_err(|_| "bad HTTP method".to_string())?;
        let encoding = req.encoding.as_deref().and_then(|l| encoding_rs::Encoding::for_label(l.as_bytes()));

        let (mut body, mut content_type) = (None::<Vec<u8>>, None::<&str>);
        if let Some(form) = &req.form {
            body = Some(encode_form(form, encoding).into_bytes());
            content_type = Some("application/x-www-form-urlencoded");
        } else if let Some(json) = &req.json {
            body = Some(json.to_string().into_bytes());
            content_type = Some("application/json");
        } else if let Some(b) = &req.body {
            body = Some(b.clone().into_bytes());
        }
        if body.is_some() && method == Method::GET {
            method = Method::POST;
        }
        let timeout = req
            .timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_TIMEOUT)
            .min(MAX_TIMEOUT);
        let max_bytes = req.max_bytes.unwrap_or(DEFAULT_MAX_BYTES).min(64 << 20);

        for _ in 0..=MAX_REDIRECTS {
            if !policy.allows(&url) {
                return Err(format!(
                    "the plugin is not allowed to connect to {}",
                    url.host_str().unwrap_or("this address")
                ));
            }
            let mut rb = client.request(method.clone(), url.clone()).timeout(timeout);
            for (k, v) in &req.headers {
                let name = HeaderName::from_bytes(k.as_bytes()).map_err(|_| format!("bad header name {k:?}"))?;
                let value = HeaderValue::from_str(v).map_err(|_| format!("bad value for header {k}"))?;
                rb = rb.header(name, value);
            }
            if let Some(b) = &body {
                if let Some(ct) = content_type {
                    if !req.headers.keys().any(|k| k.eq_ignore_ascii_case("content-type")) {
                        rb = rb.header(CONTENT_TYPE, ct);
                    }
                }
                rb = rb.body(b.clone());
            }
            let resp = rb.send().map_err(|e| format!("request to {} failed: {e}", url.host_str().unwrap_or("?")))?;
            let status = resp.status();

            if status.is_redirection() {
                if let Some(loc) = resp.headers().get(LOCATION).and_then(|v| v.to_str().ok()) {
                    url = url.join(loc).map_err(|e| format!("bad redirect: {e}"))?;
                    if matches!(status.as_u16(), 301 | 302 | 303) {
                        method = Method::GET;
                        body = None;
                    }
                    continue;
                }
            }

            let headers: BTreeMap<String, String> = resp
                .headers()
                .iter()
                .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.as_str().to_lowercase(), v.to_string())))
                .collect();
            let final_url = resp.url().to_string();
            let mut data = Vec::new();
            resp.take(max_bytes + 1)
                .read_to_end(&mut data)
                .map_err(|e| format!("could not read the response: {e}"))?;
            if data.len() as u64 > max_bytes {
                return Err(format!("the response is larger than {} MB", max_bytes >> 20));
            }
            return Ok(if req.binary {
                Response {
                    status: status.as_u16(),
                    ok: status.is_success(),
                    url: final_url,
                    headers,
                    text: String::new(),
                    base64: Some(base64::engine::general_purpose::STANDARD.encode(&data)),
                }
            } else {
                let charset = headers.get("content-type").and_then(|ct| charset_of(ct));
                let decoder = charset.or(encoding).unwrap_or(encoding_rs::UTF_8);
                Response {
                    status: status.as_u16(),
                    ok: status.is_success(),
                    url: final_url,
                    headers,
                    text: decoder.decode(&data).0.into_owned(),
                    base64: None,
                }
            });
        }
        Err("too many redirects".into())
    }
}

fn charset_of(content_type: &str) -> Option<&'static encoding_rs::Encoding> {
    let lower = content_type.to_lowercase();
    let label = lower.split("charset=").nth(1)?.split(';').next()?.trim().trim_matches('"');
    encoding_rs::Encoding::for_label(label.as_bytes())
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// `application/x-www-form-urlencoded`, in the given charset (some trackers still expect 1251).
fn encode_form(form: &BTreeMap<String, Value>, encoding: Option<&'static encoding_rs::Encoding>) -> String {
    fn escape(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|&b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
                b' ' => "+".to_string(),
                _ => format!("%{b:02X}"),
            })
            .collect()
    }
    let enc = encoding.unwrap_or(encoding_rs::UTF_8);
    form.iter()
        .map(|(k, v)| format!("{}={}", escape(&enc.encode(k).0), escape(&enc.encode(&value_to_string(v)).0)))
        .collect::<Vec<_>>()
        .join("&")
}

#[cfg(test)]
mod unit {
    use super::*;

    fn policy(hosts: &[&str]) -> HostPolicy {
        HostPolicy::new(&hosts.iter().map(|h| h.to_string()).collect::<Vec<_>>(), &Values::new())
    }

    #[test]
    fn host_policy_matches_hosts_and_subdomains_only() {
        let p = policy(&["example.org"]);
        assert!(p.allows(&Url::parse("https://example.org/x").unwrap()));
        assert!(p.allows(&Url::parse("http://api.example.org/").unwrap()));
        assert!(!p.allows(&Url::parse("https://badexample.org/").unwrap()));
        assert!(!p.allows(&Url::parse("https://example.org.evil.com/").unwrap()));
        assert!(!p.allows(&Url::parse("file:///etc/passwd").unwrap()));
    }

    #[test]
    fn setting_host_is_taken_from_the_settings() {
        let mut s = Values::new();
        s.insert("baseUrl".into(), "http://192.168.1.5:9117/".into());
        let p = HostPolicy::new(&["$setting:baseUrl".to_string()], &s);
        assert!(p.allows(&Url::parse("http://192.168.1.5:9117/api").unwrap()));
        assert!(!p.allows(&Url::parse("http://192.168.1.6/").unwrap()));
        // Not configured: nothing is allowed.
        assert!(!HostPolicy::new(&["$setting:baseUrl".to_string()], &Values::new()).allows(&Url::parse("http://x.org").unwrap()));
    }

    #[test]
    fn forms_are_encoded_in_the_requested_charset() {
        let mut form = BTreeMap::new();
        form.insert("login_username".to_string(), Value::String("вход".into()));
        form.insert("n".to_string(), Value::from(5));
        let enc = encoding_rs::Encoding::for_label(b"windows-1251");
        assert_eq!(encode_form(&form, enc), "login_username=%E2%F5%EE%E4&n=5");
        assert_eq!(encode_form(&form, None), "login_username=%D0%B2%D1%85%D0%BE%D0%B4&n=5");
    }
}
