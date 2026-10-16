// Bitsearch (formerly SolidTorrents). Based on qBittorrent's solidtorrents plugin, rewritten for
// the current site layout.
definePlugin({
  id: "bitsearch",
  name: "Bitsearch",
  version: "1.0.0",
  description: {
    en: "Torrent search engine, the former SolidTorrents (no login).",
    ru: "Поисковик по торрентам, бывший SolidTorrents (без входа).",
  },
  author: "piTorrent",
  // The site keeps moving: solidtorrents.to -> bitsearch.to -> bitsearch.eu.
  hosts: ["solidtorrents.to", "bitsearch.to", "bitsearch.eu"],
  categories: ["music", "books"],
  settings: [],

  search: function (ctx, query, opts) {
    var CATS = { music: "Audio", books: "eBook" };
    var base = "https://solidtorrents.to";
    var out = [];
    // These sites turn away clients that do not look like a browser.
    var headers = { "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0" };
    for (var page = 1; page <= 3; page++) {
      var url = base + "/search?" + new URLSearchParams({ q: query, category: (opts.category && CATS[opts.category]) || "all", sort: "seeders", page: page }).toString();
      var r = ctx.http.get(url, { headers: headers });
      if (!r.ok) {
        if (page > 1) break;
        throw new Error("solidtorrents.to answered " + r.status);
      }
      // Follow the site to wherever it has moved.
      var origin = /^https?:\/\/[^\/]+/.exec(r.url || "");
      if (origin) base = origin[0];
      var cards = ctx.html.parse(r.text).select(".torrent-result-card");
      cards.forEach(function (card) {
        var a = card.first("h3 a");
        var magnet = card.first("a[href^='magnet:']");
        if (!a || !magnet) return;
        var meta = card.select(".torrent-result-meta");
        // First block: category, size, date (M/D/YYYY). Second block: seeders, leechers, downloads.
        var info = meta[0] ? meta[0].select("span span") : [];
        var stats = meta[1] ? meta[1].select(".font-medium") : [];
        var d = info[2] ? /(\d+)\/(\d+)\/(\d+)/.exec(info[2].text()) : null;
        out.push({
          title: a.text(),
          size: info[1] ? ctx.util.sizeToBytes(info[1].text()) : null,
          seeders: stats[0] ? ctx.util.number(stats[0].text()) : null,
          leechers: stats[1] ? ctx.util.number(stats[1].text()) : null,
          date: d ? d[3] + "-" + ("0" + d[1]).slice(-2) + "-" + ("0" + d[2]).slice(-2) : null,
          magnet: magnet.attr("href"),
          pageUrl: base + a.attr("href"),
          category: info[0] ? info[0].text() : null,
        });
      });
      if (cards.length < 15) break;
    }
    return out;
  },
});
