// LimeTorrents (HTML). Port of qBittorrent's limetorrents plugin. The info hash is read from the
// download link, so the magnet link can be built without opening the details page.
definePlugin({
  id: "limetorrents",
  name: "LimeTorrents",
  version: "1.0.0",
  description: {
    en: "General-purpose public tracker (no login).",
    ru: "Универсальный открытый трекер (без входа).",
  },
  author: "piTorrent",
  hosts: ["limetorrents.lol", "limetorrents.fun", "limetorrents.info"],
  categories: ["anime", "software", "games", "movies", "music", "tv"],
  settings: [],

  search: function (ctx, query, opts) {
    var CATS = { anime: "anime", software: "applications", games: "games", movies: "movies", music: "music", tv: "tv" };
    var cat = (opts.category && CATS[opts.category]) || "all";
    var base = "https://www.limetorrents.lol";
    var out = [];
    // These sites turn away clients that do not look like a browser.
    var headers = { "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0" };
    for (var page = 1; page <= 3; page++) {
      var url = base + "/search/" + cat + "/" + encodeURIComponent(query).replace(/%20/g, "-") + "/seeds/" + page + "/";
      var r = ctx.http.get(url, { headers: headers });
      if (!r.ok) {
        if (page > 1) break;
        throw new Error("limetorrents.lol answered " + r.status);
      }
      var rows = ctx.html.parse(r.text).select("table.table2 tr[bgcolor]");
      rows.forEach(function (row) {
        var cells = row.select("td");
        if (cells.length < 5) return;
        var links = cells[0].select("a");
        var hash = /torrent\/([0-9A-Fa-f]{40})/.exec(links.length ? links[0].attr("href") || "" : "");
        var title = links.filter(function (a) { return /\.html$/.test(a.attr("href") || ""); })[0];
        if (!hash || !title) return;
        var kind = /in\s+(.+?)\s*$/.exec(cells[1].text());
        out.push({
          title: title.text(),
          size: ctx.util.sizeToBytes(cells[2].text()),
          seeders: ctx.util.number(cells[3].text()),
          leechers: ctx.util.number(cells[4].text()),
          magnet: ctx.util.magnet(hash[1], title.text()),
          pageUrl: base + title.attr("href"),
          category: kind ? kind[1].toLowerCase() : null,
        });
      });
      if (rows.length < 20) break;
    }
    return out;
  },
});
