// Rutor: a Russian public tracker (no login). Magnet links come straight from the result list.
definePlugin({
  id: "rutor",
  name: "Rutor",
  version: "1.0.0",
  description: {
    en: "Russian public tracker: films, series, music, games, software (no login).",
    ru: "Открытый русскоязычный трекер: фильмы, сериалы, музыка, игры, программы (без входа).",
  },
  author: "piTorrent",
  hosts: ["rutor.info", "rutor.is", "rutor.org"],
  categories: ["movies", "tv", "anime", "music", "games", "software", "books"],
  settings: [
    {
      key: "baseUrl",
      label: { en: "Mirror", ru: "Зеркало" },
      type: "select",
      default: "https://rutor.info",
      options: [
        { value: "https://rutor.info", label: "rutor.info" },
        { value: "https://rutor.is", label: "rutor.is" },
        { value: "https://rutor.org", label: "rutor.org" },
      ],
      help: { en: "Use another one if the default is blocked.", ru: "Выберите другое, если основное заблокировано." },
    },
  ],

  search: function (ctx, query, opts) {
    var CATS = { movies: 1, tv: 4, anime: 10, music: 2, games: 8, software: 9, books: 11 };
    var MONTHS = ["янв", "фев", "мар", "апр", "май", "июн", "июл", "авг", "сен", "окт", "ноя", "дек"];
    var base = ctx.settings.baseUrl || "https://rutor.info";
    var cat = (opts.category && CATS[opts.category]) || 0;
    var headers = { "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0" };
    var out = [];
    // /search/<page>/<category>/<mode: 000 = in titles>/<sort: 2 = by seeders>/<text>
    for (var page = 0; page < 3; page++) {
      var r = ctx.http.get(base + "/search/" + page + "/" + cat + "/000/2/" + encodeURIComponent(query), { headers: headers });
      if (!r.ok) {
        if (page > 0) break;
        throw new Error(base + " answered " + r.status);
      }
      var rows = ctx.html.parse(r.text).select("#index tr.gai, #index tr.tum");
      rows.forEach(function (row) {
        var cells = row.select("td");
        if (cells.length < 4) return;
        var magnet = cells[1].first("a[href^='magnet:']");
        var links = cells[1].select("a");
        var title = links[links.length - 1];
        if (!magnet || !title) return;
        var name = title.text();
        var d = /(\d+)\s+(\S+)\s+(\d+)/.exec(cells[0].text());
        var month = d ? MONTHS.indexOf(d[2].slice(0, 3).toLowerCase()) : -1;
        out.push({
          title: name,
          // Rows with comments have an extra cell, so count from the end.
          size: ctx.util.sizeToBytes(cells[cells.length - 2].text()),
          seeders: ctx.util.number(row.first("span.green") ? row.first("span.green").text() : ""),
          leechers: ctx.util.number(row.first("span.red") ? row.first("span.red").text() : ""),
          date: month >= 0 ? "20" + ("0" + d[3]).slice(-2) + "-" + ("0" + (month + 1)).slice(-2) + "-" + ("0" + d[1]).slice(-2) : null,
          // The link carries a placeholder name, so put the real one in.
          magnet: magnet.attr("href").replace(/dn=[^&]*/, "dn=" + encodeURIComponent(name)),
          pageUrl: base + title.attr("href"),
        });
      });
      if (rows.length < 100) break;
    }
    return out;
  },
});
