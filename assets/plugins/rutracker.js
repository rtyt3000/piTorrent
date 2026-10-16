// RuTracker (needs an account). Signs in with the login and password, or uses a ready cookie.
//
// The site is often behind a Cloudflare check that a plain client cannot pass. In that case
// copy the cookies from a browser where the site is open (bb_session and cf_clearance) into the
// "Cookie" setting; the browser's User-Agent must then match the one below.
definePlugin({
  id: "rutracker",
  name: "RuTracker",
  version: "1.0.0",
  description: {
    en: "The largest Russian tracker. Needs an account.",
    ru: "Крупнейший русскоязычный трекер. Нужна учётная запись.",
  },
  author: "piTorrent",
  hosts: ["$setting:baseUrl"],
  categories: [],
  settings: [
    { key: "baseUrl", label: { en: "Address", ru: "Адрес" }, type: "url", default: "https://rutracker.org", required: true,
      help: { en: "A mirror can be used, e.g. https://rutracker.net", ru: "Можно указать зеркало, например https://rutracker.net" } },
    { key: "username", label: { en: "Login", ru: "Логин" }, type: "text", required: true },
    { key: "password", label: { en: "Password", ru: "Пароль" }, type: "password", required: true },
    { key: "cookie", label: "Cookie", type: "cookie",
      help: { en: "Optional. For example: bb_session=...; cf_clearance=... Used instead of the login when set.",
              ru: "Необязательно. Например: bb_session=...; cf_clearance=... Если задано, вход по логину не выполняется." } },
  ],

  check: function (ctx) {
    var s = session(ctx);
    var r = ctx.http.get(s.base + "/forum/tracker.php", { headers: s.headers, encoding: "windows-1251" });
    if (!r.ok) throw new Error("the site answered " + r.status);
    if (isLoginPage(r.text)) throw new Error("not signed in: check the login, the password or the cookie");
    return "Signed in";
  },

  search: function (ctx, query) {
    var s = session(ctx);
    var out = [];
    // The first request creates a search on the site; the other pages are reached through the
    // page links it returns (they carry the search id), at most 3 pages to be polite.
    var url = s.base + "/forum/tracker.php?" + new URLSearchParams({ nm: query }).toString();
    var seen = {};
    for (var page = 0; page < 3 && url; page++) {
      var doc = ctx.html.parse(fetchPage(ctx, s, url).text);
      doc.select("tr[id^='trs-tr-']").forEach(function (row) {
        var link = row.first("a[data-topic_id]");
        if (!link) return;
        var id = link.attr("data-topic_id");
        // data-ts_text holds raw values, in this order: size, seeders, added (unix time).
        var raw = row.select("[data-ts_text]").map(function (el) { return el.attr("data-ts_text"); });
        var leech = row.first(".leechmed");
        var forum = row.first("a.f");
        out.push({
          title: link.text(),
          size: raw[0] ? Number(raw[0]) : null,
          // Negative when the release is not available for download.
          seeders: raw[1] ? Math.max(0, Number(raw[1])) : null,
          leechers: leech ? ctx.util.number(leech.text()) : null,
          date: /^\d{10}$/.test(raw[2] || "") ? new Date(Number(raw[2]) * 1000).toISOString().slice(0, 10) : null,
          // The .torrent needs the session, so resolve() downloads it.
          pageUrl: s.base + "/forum/viewtopic.php?t=" + id,
          category: forum ? forum.text() : null,
        });
      });
      seen[url] = true;
      url = null;
      doc.select("a[href^='tracker.php?']").forEach(function (a) {
        var href = a.attr("href");
        var next = s.base + "/forum/" + href.replace(/&amp;/g, "&");
        if (!url && /[?&]start=\d+/.test(href) && !seen[next]) url = next;
      });
    }
    return out;
  },

  resolve: function (ctx, result) {
    var s = session(ctx);
    var id = /t=(\d+)/.exec(result.pageUrl || "");
    if (!id) throw new Error("no topic id in the link");
    // The login must have happened before the download.
    fetchPage(ctx, s, s.base + "/forum/tracker.php");
    var headers = Object.assign({ Referer: result.pageUrl }, s.headers);
    var r = ctx.http.request({ url: s.base + "/forum/dl.php?t=" + id[1], method: "GET", headers: headers, binary: true });
    if (!r.ok) throw new Error("the site answered " + r.status);
    return { torrentBase64: r.base64 };
  },
});

var UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0";

function session(ctx) {
  var headers = { "User-Agent": UA };
  if (ctx.settings.cookie) headers.Cookie = ctx.settings.cookie;
  return { base: String(ctx.settings.baseUrl || "https://rutracker.org").replace(/\/+$/, ""), headers: headers, cookie: !!ctx.settings.cookie };
}

function isLoginPage(text) {
  return text.indexOf('name="login_username"') >= 0 || text.indexOf("id=\"top-login-form\"") >= 0;
}

function login(ctx, s) {
  var r = ctx.http.post(
    s.base + "/forum/login.php",
    { login_username: ctx.settings.username, login_password: ctx.settings.password, login: "Вход" },
    { headers: s.headers, encoding: "windows-1251" }
  );
  if (!r.ok) throw new Error("the site answered " + r.status);
  if (isLoginPage(r.text)) throw new Error("the login or the password was not accepted (or a captcha is required: use the cookie instead)");
  ctx.state.set("loggedIn", true);
}

// Loads a page, signing in first when there is no cookie and the session is missing or expired.
function fetchPage(ctx, s, url) {
  if (!s.cookie && !ctx.state.get("loggedIn")) login(ctx, s);
  var r = ctx.http.get(url, { headers: s.headers, encoding: "windows-1251" });
  if (r.ok && isLoginPage(r.text) && !s.cookie) {
    login(ctx, s);
    r = ctx.http.get(url, { headers: s.headers, encoding: "windows-1251" });
  }
  if (!r.ok) throw new Error("the site answered " + r.status);
  if (isLoginPage(r.text)) throw new Error("not signed in: check the login, the password or the cookie");
  return r;
}
