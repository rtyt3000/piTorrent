// Loaded before every plugin. Builds the `ctx` object that plugins receive and hides the
// native functions behind it. Plugins are synchronous: every call below returns its result.
(function (g) {
  var N = g.__native;
  delete g.__native;

  var def = null;
  g.definePlugin = function (d) {
    if (def) throw new Error("definePlugin() was called twice");
    def = d;
  };

  function fmt(args) {
    return Array.prototype.map
      .call(args, function (x) {
        return typeof x === "string" ? x : JSON.stringify(x);
      })
      .join(" ");
  }
  g.console = {
    log: function () { N.log("info", fmt(arguments)); },
    info: function () { N.log("info", fmt(arguments)); },
    warn: function () { N.log("warn", fmt(arguments)); },
    error: function () { N.log("error", fmt(arguments)); },
    debug: function () { N.log("debug", fmt(arguments)); },
  };

  g.URLSearchParams = function (init) {
    this.p = [];
    var self = this;
    if (typeof init === "string") {
      init.replace(/^\?/, "").split("&").forEach(function (kv) {
        if (!kv) return;
        var i = kv.indexOf("=");
        var k = i < 0 ? kv : kv.slice(0, i);
        var v = i < 0 ? "" : kv.slice(i + 1);
        self.p.push([decodeURIComponent(k.replace(/\+/g, " ")), decodeURIComponent(v.replace(/\+/g, " "))]);
      });
    } else if (init) {
      Object.keys(init).forEach(function (k) { self.p.push([k, String(init[k])]); });
    }
  };
  g.URLSearchParams.prototype.append = function (k, v) { this.p.push([k, String(v)]); };
  g.URLSearchParams.prototype.get = function (k) {
    for (var i = 0; i < this.p.length; i++) if (this.p[i][0] === k) return this.p[i][1];
    return null;
  };
  g.URLSearchParams.prototype.toString = function () {
    return this.p.map(function (kv) { return encodeURIComponent(kv[0]) + "=" + encodeURIComponent(kv[1]); }).join("&");
  };

  var UNITS = { b: 1, kb: 1024, kib: 1024, mb: 1048576, mib: 1048576, gb: 1073741824, gib: 1073741824, tb: 1099511627776, tib: 1099511627776,
    "б": 1, "кб": 1024, "мб": 1048576, "гб": 1073741824, "тб": 1099511627776 };
  var util = {
    // "1.5 GB", "700 MiB", "1,5 ГБ" -> bytes
    sizeToBytes: function (s) {
      var m = /([\d.,]+)\s*([a-zA-Zа-яА-Я]+)?/.exec(String(s));
      if (!m) return null;
      var n = parseFloat(m[1].replace(",", "."));
      var unit = (m[2] || "b").toLowerCase();
      return isNaN(n) || !(unit in UNITS) ? null : Math.round(n * UNITS[unit]);
    },
    // "1,234" / "1 234" -> 1234
    number: function (s) {
      var d = String(s).replace(/[^\d]/g, "");
      return d ? parseInt(d, 10) : null;
    },
    magnet: function (infoHash, name, trackers) {
      var m = "magnet:?xt=urn:btih:" + infoHash;
      if (name) m += "&dn=" + encodeURIComponent(name);
      (trackers || []).forEach(function (t) { m += "&tr=" + encodeURIComponent(t); });
      return m;
    },
  };

  // An element found by select(); `doc` and `rec.id` are handles into the parsed document.
  function node(doc, rec) {
    return {
      text: function () { return rec.t; },
      html: function () { return rec.h; },
      attr: function (n) { return n in rec.a ? rec.a[n] : null; },
      select: function (css) { return select(doc, rec.id, css); },
      first: function (css) { return select(doc, rec.id, css)[0] || null; },
    };
  }
  function select(doc, id, css) {
    var out = JSON.parse(N.select(doc, id, String(css)));
    if (out.error) throw new Error(out.error);
    return out.map(function (rec) { return node(doc, rec); });
  }
  var html = {
    parse: function (src) {
      var doc = N.parse(String(src));
      return {
        select: function (css) { return select(doc, -1, css); },
        first: function (css) { return select(doc, -1, css)[0] || null; },
      };
    },
  };

  var http = {
    request: function (o) {
      var r = JSON.parse(N.http(JSON.stringify(o)));
      if (r.error) throw new Error(r.error);
      r.json = function () { return JSON.parse(r.text); };
      return r;
    },
    get: function (url, o) { return http.request(Object.assign({}, o, { url: url, method: "GET" })); },
    post: function (url, form, o) { return http.request(Object.assign({}, o, { url: url, method: "POST", form: form })); },
    postJson: function (url, json, o) { return http.request(Object.assign({}, o, { url: url, method: "POST", json: json })); },
  };

  g.__meta = function () {
    if (!def) throw new Error("the script did not call definePlugin()");
    return JSON.stringify({
      id: def.id,
      name: def.name,
      version: def.version,
      description: def.description,
      author: def.author,
      icon: def.icon,
      hosts: def.hosts,
      categories: def.categories,
      settings: def.settings,
      hasResolve: typeof def.resolve === "function",
      hasCheck: typeof def.check === "function",
    });
  };

  g.__call = function (method, ctxJson, argsJson) {
    if (!def) throw new Error("the script did not call definePlugin()");
    var c = JSON.parse(ctxJson);
    var state = c.state || {};
    var ctx = {
      settings: c.settings || {},
      http: http,
      html: html,
      util: util,
      log: function () { N.log("info", fmt(arguments)); },
      state: {
        get: function (k) { return state[k]; },
        set: function (k, v) { state[k] = v; N.statePut(JSON.stringify(state)); },
      },
    };
    var fn = def[method];
    if (typeof fn !== "function") throw new Error("the plugin has no " + method + "()");
    var out = fn.apply(def, [ctx].concat(JSON.parse(argsJson)));
    return JSON.stringify(out === undefined ? null : out);
  };
})(globalThis);
