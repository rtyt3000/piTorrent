use super::host::{self, Env};
use super::http::HostPolicy;
use super::*;
use serde_json::json;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A tiny HTTP server that answers fixed responses and records the requests it gets.
struct Mock {
    addr: SocketAddr,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Mock {
    fn new(routes: Vec<(&'static str, Vec<u8>)>) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
                loop {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(head_end) = text.find("\r\n\r\n") {
                        let len = text
                            .lines()
                            .find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                            .unwrap_or(0);
                        if buf.len() >= head_end + 4 + len {
                            break;
                        }
                    }
                }
                let request = String::from_utf8_lossy(&buf).to_string();
                let path = request.split_whitespace().nth(1).unwrap_or("/").to_string();
                log.lock().unwrap().push(request);
                let response = routes
                    .iter()
                    .find(|(prefix, _)| path.starts_with(prefix))
                    .map(|(_, r)| r.clone())
                    .unwrap_or_else(|| ok(b"not found", "text/plain").replace_status("404 Not Found"));
                let _ = stream.write_all(&response);
            }
        });
        Mock { addr, seen }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn requests(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
}

trait ReplaceStatus {
    fn replace_status(self, status: &str) -> Vec<u8>;
}

impl ReplaceStatus for Vec<u8> {
    fn replace_status(self, status: &str) -> Vec<u8> {
        let text = String::from_utf8_lossy(&self).replacen("200 OK", status, 1);
        text.into_bytes()
    }
}

fn response(status: &str, headers: &[&str], body: &[u8]) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
    for h in headers {
        out.push_str(h);
        out.push_str("\r\n");
    }
    out.push_str("\r\n");
    let mut bytes = out.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

fn ok(body: &[u8], content_type: &str) -> Vec<u8> {
    response("200 OK", &[&format!("Content-Type: {content_type}")], body)
}

fn cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn plugin(src: &str) -> Plugin {
    Plugin::from_source(src, None, false).unwrap_or_else(|e| panic!("plugin did not load: {e}"))
}

fn settings(pairs: &[(&str, &str)]) -> Values {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

const LOCAL: &str = r#"
definePlugin({
  id: "t", name: "T", hosts: ["127.0.0.1"], categories: ["movies"],
  settings: [{ key: "base", label: "Base", type: "url" }],
  search: function (ctx, q, opts) {
    var r = ctx.http.get(ctx.settings.base + "/s", { query: { q: q } });
    return r.json().map(function (x) { return { title: x.n, size: x.s, seeders: x.se, magnet: x.m }; });
  },
});
"#;

#[test]
fn reads_the_declaration_of_the_example_plugin() {
    let meta = host::read_meta(include_str!("../../assets/plugins/example.js")).unwrap();
    assert_eq!(meta.id, "example");
    assert_eq!(meta.settings.len(), 8);
    assert!(meta.has_resolve && meta.has_check);
    let kinds: Vec<_> = meta.settings.iter().map(|f| f.kind).collect();
    assert!(kinds.contains(&FieldKind::Password) && kinds.contains(&FieldKind::Cookie) && kinds.contains(&FieldKind::Bool));
    assert!(meta.settings.iter().find(|f| f.key == "cookie").unwrap().is_secret());
    assert!(!meta.settings.iter().find(|f| f.key == "username").unwrap().is_secret());
}

#[test]
fn rejects_scripts_without_a_valid_declaration() {
    assert!(host::read_meta("var x = 1;").unwrap_err().0.contains("definePlugin"));
    assert!(host::read_meta("definePlugin({ id: 'Bad Id', name: 'x', search: function(){} })").unwrap_err().0.contains("id"));
    assert!(host::read_meta("definePlugin({ id: 'ok', name: ' ' })").unwrap_err().0.contains("name"));
    assert!(host::read_meta("definePlugin({ id: 'ok', name: 'x', settings: [{key:'a'},{key:'a'}] })").is_err());
    assert!(host::read_meta("this is not javascript").is_err());
}

#[test]
fn search_results_are_read_leniently() {
    let body = br#"[{"n":"Alpha","s":"1500","se":"12","m":"magnet:?xt=urn:btih:aa"},
                    {"n":"","s":1,"m":"magnet:?xt=urn:btih:bb"},
                    {"n":"No link","s":2},
                    {"n":"Bad magnet","m":"http://not-a-magnet"}]"#;
    let mock = Mock::new(vec![("/s", ok(body, "application/json"))]);
    let p = plugin(LOCAL);
    let results = p.search(settings(&[("base", &mock.url())]), "linux", None, cancel()).unwrap();
    assert_eq!(results.len(), 1, "only the usable result survives: {results:?}");
    assert_eq!(results[0].title, "Alpha");
    assert_eq!(results[0].size, Some(1500));
    assert_eq!(results[0].seeders, Some(12));
    assert_eq!((results[0].plugin_id.as_str(), results[0].plugin_name.as_str()), ("t", "T"));
    assert!(mock.requests()[0].starts_with("GET /s?q=linux "));
}

#[test]
fn a_plugin_cannot_reach_hosts_it_did_not_declare() {
    let mock = Mock::new(vec![("/", ok(b"[]", "application/json"))]);
    let src = r#"definePlugin({ id: "t", name: "T", hosts: ["example.org"],
        search: function (ctx) { ctx.http.get("BASE/s"); return []; } });"#
        .replace("BASE", &mock.url());
    let err = plugin(&src).search(Values::new(), "x", None, cancel()).unwrap_err();
    assert!(err.0.contains("not allowed"), "{err}");
    assert!(mock.requests().is_empty(), "no request may leave the sandbox");
}

#[test]
fn redirects_to_other_hosts_are_blocked() {
    let target = Mock::new(vec![("/", ok(b"secret", "text/plain"))]);
    let redirect = response("302 Found", &[&format!("Location: http://localhost:{}/", target.addr.port())], b"");
    let mock = Mock::new(vec![("/", redirect)]);
    let src = r#"definePlugin({ id: "t", name: "T", hosts: ["127.0.0.1"],
        search: function (ctx) { ctx.http.get("BASE/"); return []; } });"#
        .replace("BASE", &mock.url());
    let err = plugin(&src).search(Values::new(), "x", None, cancel()).unwrap_err();
    assert!(err.0.contains("not allowed"), "{err}");
    assert!(target.requests().is_empty());
}

#[test]
fn only_http_and_https_are_allowed() {
    let src = r#"definePlugin({ id: "t", name: "T", hosts: ["*"],
        search: function (ctx) { ctx.http.get("file:///c:/windows/win.ini"); return []; } });"#;
    assert!(plugin(src).search(Values::new(), "x", None, cancel()).is_err());
    let policy = HostPolicy::new(&["127.0.0.1".to_string()], &Values::new());
    assert!(!policy.allows(&"ftp://127.0.0.1/".parse().unwrap()));
}

#[test]
fn cookies_from_a_login_are_reused_by_later_searches() {
    let mock = Mock::new(vec![
        ("/login", response("200 OK", &["Set-Cookie: sid=abc123; Path=/"], b"ok")),
        ("/data", ok(b"[{\"n\":\"Secret\",\"m\":\"magnet:?xt=urn:btih:cc\"}]", "application/json")),
    ]);
    let src = r#"definePlugin({ id: "t", name: "T", hosts: ["127.0.0.1"],
        settings: [{ key: "base", type: "url" }],
        search: function (ctx) {
          if (!ctx.state.get("in")) { ctx.http.post(ctx.settings.base + "/login", { user: "u", pass: "p" }); ctx.state.set("in", true); }
          return ctx.http.get(ctx.settings.base + "/data").json().map(function (x) { return { title: x.n, magnet: x.m }; });
        } });"#;
    let p = plugin(src);
    let s = settings(&[("base", &mock.url())]);
    assert_eq!(p.search(s.clone(), "a", None, cancel()).unwrap().len(), 1);
    assert_eq!(p.search(s, "b", None, cancel()).unwrap().len(), 1);

    let seen = mock.requests();
    let logins = seen.iter().filter(|r| r.starts_with("POST /login")).count();
    assert_eq!(logins, 1, "the login is not repeated thanks to the saved state");
    let data: Vec<_> = seen.iter().filter(|r| r.starts_with("GET /data")).collect();
    assert_eq!(data.len(), 2);
    assert!(data.iter().all(|r| r.to_lowercase().contains("cookie: sid=abc123")), "the session cookie is sent");
    assert!(seen[0].contains("user=u&pass=p") || seen[0].contains("pass=p&user=u"));
}

#[test]
fn legacy_encodings_work_both_ways() {
    // "Привет" in windows-1251
    let page = [0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2];
    let mock = Mock::new(vec![("/", ok(&page, "text/html; charset=windows-1251"))]);
    let src = r#"definePlugin({ id: "t", name: "T", hosts: ["127.0.0.1"],
        search: function (ctx) {
          ctx.http.post("BASE/login", { login_username: "вход" }, { encoding: "windows-1251" });
          var t = ctx.http.get("BASE/page").text;
          return [{ title: t, pageUrl: "http://127.0.0.1/x" }];
        } });"#
        .replace("BASE", &mock.url());
    let results = plugin(&src).search(Values::new(), "x", None, cancel()).unwrap();
    assert_eq!(results[0].title, "Привет");
    let post = mock.requests().into_iter().find(|r| r.starts_with("POST")).unwrap();
    assert!(post.contains("login_username=%E2%F5%EE%E4"), "{post}");
}

#[test]
fn html_pages_can_be_scraped_with_css_selectors() {
    let page = br#"<html><body><table>
        <tr class="row"><td class="n"><a href="/t/1">Ubuntu 24.04</a></td><td class="s">5.8 GB</td><td class="se">1,234</td></tr>
        <tr class="row"><td class="n"><a href="/t/2">Debian &amp; Co</a></td><td class="s">700 MiB</td><td class="se">9</td></tr>
        <tr class="ad"><td>ignore me</td></tr></table></body></html>"#;
    let mock = Mock::new(vec![("/", ok(page, "text/html"))]);
    let src = r#"definePlugin({ id: "t", name: "T", hosts: ["127.0.0.1"],
        search: function (ctx, q) {
          var doc = ctx.html.parse(ctx.http.get("BASE/").text);
          return doc.select("tr.row").map(function (row) {
            var a = row.first("td.n a");
            return { title: a.text(), pageUrl: "BASE" + a.attr("href"),
                     size: ctx.util.sizeToBytes(row.first("td.s").text()),
                     seeders: ctx.util.number(row.first("td.se").text()) };
          });
        } });"#
        .replace("BASE", &mock.url());
    let r = plugin(&src).search(Values::new(), "x", None, cancel()).unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!((r[0].title.as_str(), r[0].size, r[0].seeders), ("Ubuntu 24.04", Some(6_227_702_579), Some(1234)));
    assert_eq!((r[1].title.as_str(), r[1].size), ("Debian & Co", Some(734_003_200)));
    assert!(r[0].page_url.as_deref().unwrap().ends_with("/t/1"));
}

#[test]
fn runaway_scripts_are_interrupted() {
    let src = "definePlugin({ id: 't', name: 'T', search: function () { while (true) {} } });";
    let p = plugin(src);
    let env = Env { timeout: Duration::from_millis(300), ..Env::bare() };
    let started = std::time::Instant::now();
    let err = host::call(src, &env, "search", json!(["x", {}])).unwrap_err();
    assert!(err.0.contains("too long"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(5));
    drop(p);
}

#[test]
fn cancelling_stops_a_running_script() {
    let src = "definePlugin({ id: 't', name: 'T', search: function () { while (true) {} } });";
    let env = Env { timeout: Duration::from_secs(60), ..Env::bare() };
    let flag = env.cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let err = host::call(src, &env, "search", json!(["x", {}])).unwrap_err();
    assert_eq!(err.0, "cancelled");
}

#[test]
fn script_errors_become_readable_messages() {
    let src = "definePlugin({ id: 't', name: 'T', search: function () { throw new Error('login failed: bad password'); } });";
    let err = plugin(src).search(Values::new(), "x", None, cancel()).unwrap_err();
    assert_eq!(err.0, "login failed: bad password");
    let src = "definePlugin({ id: 't', name: 'T', search: function () { return 5; } });";
    assert!(plugin(src).search(Values::new(), "x", None, cancel()).unwrap_err().0.contains("array"));
}

#[test]
fn memory_hogs_fail_instead_of_taking_down_the_app() {
    let src = "definePlugin({ id: 't', name: 'T', search: function () { var a = []; while (true) { a.push(new Array(1e6).fill('x')); } } });";
    let started = std::time::Instant::now();
    let env = Env { timeout: Duration::from_secs(20), ..Env::bare() };
    assert!(host::call(src, &env, "search", json!(["x", {}])).is_err());
    assert!(started.elapsed() < Duration::from_secs(20));
}

#[test]
fn plugins_cannot_see_the_filesystem_or_process() {
    let src = "definePlugin({ id: 't', name: 'T', search: function () {
        return [{ title: [typeof require, typeof process, typeof std, typeof os, typeof fetch, typeof XMLHttpRequest, typeof __native].join(','), pageUrl: 'http://x.org' }]; } });";
    let r = plugin(src).search(Values::new(), "x", None, cancel()).unwrap();
    assert_eq!(r[0].title, "undefined,undefined,undefined,undefined,undefined,undefined,undefined");
}

#[test]
fn state_and_settings_reach_the_script() {
    let src = r#"definePlugin({ id: "t", name: "T", settings: [{key:"k"}],
        search: function (ctx) {
          var n = (ctx.state.get("n") || 0) + 1; ctx.state.set("n", n);
          return [{ title: ctx.settings.k + ":" + n, pageUrl: "http://x.org" }];
        } });"#;
    let p = plugin(src);
    let s = settings(&[("k", "v")]);
    assert_eq!(p.search(s.clone(), "x", None, cancel()).unwrap()[0].title, "v:1");
    assert_eq!(p.search(s, "x", None, cancel()).unwrap()[0].title, "v:2");
}

#[test]
fn resolve_and_check_hooks() {
    let p = plugin(include_str!("../../assets/plugins/example.js"));
    let r = p.search(settings(&[("apiKey", "k")]), "linux", Some("music".into()), cancel()).unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].category.as_deref(), Some("music"));
    assert!(r[0].magnet.as_deref().unwrap().starts_with("magnet:?xt=urn:btih:"));
    let resolved = p.resolve(Values::new(), &r[1], cancel()).unwrap();
    assert!(resolved.magnet.unwrap().contains("fedcba98"));
    assert_eq!(p.check(settings(&[("apiKey", "k")]), cancel()).unwrap().as_deref(), Some("The settings look fine"));
    assert!(p.check(Values::new(), cancel()).unwrap_err().0.contains("API key"));
}

#[test]
fn settings_store_keeps_secrets_out_of_the_file() {
    let dir = std::env::temp_dir().join(format!("piTorrent-plugins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plugins.json");
    let meta = host::read_meta(include_str!("../../assets/plugins/example.js")).unwrap();

    let mut store = Store::load(Some(path.clone()), Box::new(MemorySecrets::default()));
    assert!(!store.is_enabled(&meta), "a plugin that needs an API key starts switched off");
    assert_eq!(store.missing_required(&meta), vec!["API key"]);

    store.set_value(&meta, "apiKey", "TOP-SECRET-KEY").unwrap();
    store.set_value(&meta, "cookie", "uid=1; pass=SECRET-COOKIE").unwrap();
    store.set_value(&meta, "username", "alice").unwrap();
    store.set_value(&meta, "freeOnly", "true").unwrap();

    let values = store.values(&meta);
    assert_eq!(values["apiKey"], "TOP-SECRET-KEY");
    assert_eq!(values["username"], "alice");
    assert_eq!(values["baseUrl"], "https://example.org", "defaults fill the gaps");
    assert!(store.missing_required(&meta).is_empty());
    assert!(store.set_value(&meta, "nope", "x").is_err());

    let file = std::fs::read_to_string(&path).unwrap();
    assert!(file.contains("alice"));
    assert!(!file.contains("TOP-SECRET-KEY") && !file.contains("SECRET-COOKIE"), "secrets never reach plugins.json");

    store.set_enabled(&meta.id, true);
    let reloaded = Store::load(Some(path), Box::new(MemorySecrets::default()));
    assert!(reloaded.is_enabled(&meta));
    assert_eq!(reloaded.values(&meta)["username"], "alice");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bundled_plugins_load_and_can_be_installed_and_removed() {
    let dir = std::env::temp_dir().join(format!("piTorrent-plugin-dir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::load(None, Box::new(MemorySecrets::default()));
    let mut m = Manager::load(dir.join("plugins"), store);
    assert!(m.failed.is_empty(), "{:?}", m.failed);
    let ids: Vec<_> = m.plugins.iter().map(|p| p.meta.id.as_str()).collect();
    assert_eq!(ids, ["torznab", "internet-archive", "piratebay", "torrents-csv", "limetorrents", "bitsearch", "nyaa", "example"]);
    // Only plugins that need nothing are searchable out of the box.
    let searchable: Vec<_> = m.searchable().iter().map(|p| p.meta.id.clone()).collect();
    assert_eq!(searchable, ["internet-archive", "piratebay", "torrents-csv", "limetorrents", "bitsearch", "nyaa"]);

    let file = dir.join("mine.js");
    std::fs::write(&file, "definePlugin({ id: 'mine', name: 'Mine', search: function () { return []; } });").unwrap();
    assert_eq!(m.install(&file).unwrap(), "mine");
    assert!(m.install(&file).is_err(), "the same id cannot be installed twice");
    assert!(dir.join("plugins").join("mine.js").exists());
    let again = Manager::load(dir.join("plugins"), Store::load(None, Box::new(MemorySecrets::default())));
    assert!(again.get("mine").is_some(), "installed plugins are found after a restart");

    m.remove("mine");
    assert!(m.get("mine").is_none() && !dir.join("plugins").join("mine.js").exists());
    m.remove("torznab");
    assert!(m.get("torznab").is_some(), "bundled plugins cannot be removed");

    let bad = dir.join("bad.js");
    std::fs::write(&bad, "definePlugin({ id: 'Not Valid', name: 'x' });").unwrap();
    assert!(m.install(&bad).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn torznab_plugin_talks_to_jackett() {
    let jackett = serde_json::to_vec(&json!({ "Results": [
        { "Tracker": "RuTracker", "Title": "Ubuntu 24.04", "Size": 6227702579u64, "Seeders": 40, "Peers": 55,
          "PublishDate": "2025-02-03T10:00:00", "MagnetUri": "magnet:?xt=urn:btih:aa", "Link": "http://127.0.0.1/dl/1", "Details": "http://127.0.0.1/d/1" },
        { "Tracker": "Other", "Title": "No magnet", "Size": 1, "Seeders": 0, "Peers": 0, "Link": "http://127.0.0.1/dl/2" }
    ]})).unwrap();
    let mock = Mock::new(vec![
        ("/api/v2.0/indexers/all/results", ok(&jackett, "application/json")),
        ("/api/v2.0/indexers?", ok(b"[{},{},{}]", "application/json")),
    ]);
    let p = plugin(include_str!("../../assets/plugins/torznab.js"));
    let s = settings(&[("flavor", "jackett"), ("baseUrl", &mock.url()), ("apiKey", "KEY"), ("indexer", "all")]);

    let r = p.search(s.clone(), "ubuntu", Some("software".into()), cancel()).unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!((r[0].leechers, r[0].seeders, r[0].date.as_deref()), (Some(15), Some(40), Some("2025-02-03")));
    assert_eq!(r[0].category.as_deref(), Some("RuTracker"));
    assert!(r[1].magnet.is_none() && r[1].torrent_url.is_some());
    let first = &mock.requests()[0];
    assert!(first.contains("apikey=KEY") && first.contains("Query=ubuntu") && first.contains("Category%5B%5D=4000"), "{first}");

    assert_eq!(p.check(s, cancel()).unwrap().as_deref(), Some("3 indexers"));
    // Without an address nothing may leave the machine.
    let err = p.search(settings(&[("flavor", "jackett"), ("baseUrl", ""), ("apiKey", "KEY")]), "x", None, cancel()).unwrap_err();
    assert!(!err.0.is_empty());
}

#[test]
fn torznab_plugin_talks_to_prowlarr() {
    let body = serde_json::to_vec(&json!([
        { "indexer": "Nyaa", "title": "Anime ep 1", "size": 500000000u64, "seeders": 10, "leechers": 2,
          "publishDate": "2025-03-01T00:00:00Z", "magnetUrl": "magnet:?xt=urn:btih:dd", "infoUrl": "http://127.0.0.1/i/1" }
    ])).unwrap();
    let mock = Mock::new(vec![("/api/v1/search", ok(&body, "application/json"))]);
    let p = plugin(include_str!("../../assets/plugins/torznab.js"));
    let s = settings(&[("flavor", "prowlarr"), ("baseUrl", &mock.url()), ("apiKey", "PKEY")]);
    let r = p.search(s, "anime", Some("anime".into()), cancel()).unwrap();
    assert_eq!((r[0].seeders, r[0].leechers, r[0].category.as_deref()), (Some(10), Some(2), Some("Nyaa")));
    let req = mock.requests()[0].to_lowercase();
    assert!(req.contains("x-api-key: pkey") && req.contains("categories=5070"), "{req}");
}

#[test]
fn torrent_files_are_fetched_through_the_plugin_session() {
    let mock = Mock::new(vec![
        ("/good", ok(b"d8:announce3:urle", "application/x-bittorrent")),
        ("/login-page", ok(b"<html>please log in</html>", "text/html")),
    ]);
    let p = plugin(LOCAL);
    let s = settings(&[("base", &mock.url())]);
    assert_eq!(p.fetch_torrent(s.clone(), &format!("{}/good", mock.url()), cancel()).unwrap(), b"d8:announce3:urle");
    let err = p.fetch_torrent(s.clone(), &format!("{}/login-page", mock.url()), cancel()).unwrap_err();
    assert!(err.0.contains("login"), "{err}");
    assert!(p.fetch_torrent(s, "http://evil.example.com/x.torrent", cancel()).unwrap_err().0.contains("not allowed"));
}

#[test]
fn texts_can_be_given_per_language() {
    let src = r#"definePlugin({ id: "t", name: "T",
        description: { en: "English text", ru: "Русский текст" },
        settings: [{ key: "k", label: { ru: "Только по-русски" } }, { key: "p", label: "Plain" }, { key: "x" }],
        search: function () { return []; } });"#;
    let meta = host::read_meta(src).unwrap();
    assert_eq!(meta.description.get_in("ru"), "Русский текст");
    assert_eq!(meta.description.get_in("en"), "English text");
    assert_eq!(meta.description.get_in("de"), "English text", "unknown languages fall back to English");
    let [k, p, x] = [&meta.settings[0], &meta.settings[1], &meta.settings[2]];
    assert_eq!(k.label.get_in("en"), "Только по-русски", "with no English text, any text beats nothing");
    assert_eq!(p.label.get_in("ru"), "Plain", "a plain string is used for every language");
    assert_eq!(x.title(), "x", "the key stands in for a missing label");
}

/// Talks to the real OS credential store, so it is skipped by default:
/// `cargo test real_keyring -- --ignored`
#[test]
#[ignore]
fn real_keyring_roundtrip() {
    let secrets = KeyringSecrets;
    let (plugin, key) = ("piTorrent-test-plugin", "token");
    use super::store::Secrets;
    secrets.set(plugin, key, Some("s3cret-value")).unwrap();
    assert_eq!(secrets.get(plugin, key).as_deref(), Some("s3cret-value"));
    secrets.set(plugin, key, Some("changed")).unwrap();
    assert_eq!(secrets.get(plugin, key).as_deref(), Some("changed"));
    secrets.set(plugin, key, None).unwrap();
    assert_eq!(secrets.get(plugin, key), None);
    secrets.set(plugin, key, None).unwrap(); // deleting twice is fine
}

/// Talks to the real sites, so it is skipped by default:
/// `cargo test live_bundled_searches -- --ignored --nocapture`
#[test]
#[ignore]
fn live_bundled_searches() {
    let mut failed = Vec::new();
    for (name, src) in [
        ("piratebay", include_str!("../../assets/plugins/piratebay.js")),
        ("torrents-csv", include_str!("../../assets/plugins/torrents-csv.js")),
        ("limetorrents", include_str!("../../assets/plugins/limetorrents.js")),
        ("bitsearch", include_str!("../../assets/plugins/bitsearch.js")),
        ("nyaa", include_str!("../../assets/plugins/nyaa.js")),
    ] {
        match plugin(src).search(Values::new(), "ubuntu", None, cancel()) {
            Ok(r) if !r.is_empty() => {
                println!("{name}: {} results", r.len());
                println!("   {:?}", r[0]);
            }
            Ok(_) => failed.push(format!("{name}: found nothing")),
            Err(e) => failed.push(format!("{name}: {e}")),
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}

/// Bitsearch is checked against a saved page (the site rate-limits), through a local server.
#[test]
fn bitsearch_page_is_parsed() {
    let page = include_bytes!("fixtures/bitsearch.html");
    let mock = Mock::new(vec![("/search", ok(page, "text/html"))]);
    let src = include_str!("../../assets/plugins/bitsearch.js")
        .replace("https://solidtorrents.to", &mock.url())
        .replace("[\"solidtorrents.to\", \"bitsearch.to\"]", "[\"127.0.0.1\"]");
    let r = plugin(&src).search(Values::new(), "ubuntu", None, cancel()).unwrap();
    assert!(!r.is_empty());
    let first = &r[0];
    assert!(first.title.contains("ubuntu"), "{first:?}");
    assert!(first.size.is_some() && first.seeders.is_some() && first.leechers.is_some(), "{first:?}");
    assert!(first.magnet.as_deref().is_some_and(|m| m.starts_with("magnet:?xt=urn:btih:")), "{first:?}");
    assert!(first.date.as_deref().is_some_and(|d| d.len() == 10), "{first:?}");
}
