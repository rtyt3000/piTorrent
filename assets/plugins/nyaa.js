// Nyaa.si: anime, manga, music and software. Based on the unofficial qBittorrent nyaa plugin.
definePlugin({
  id: "nyaa",
  name: "Nyaa",
  version: "1.0.0",
  description: {
    en: "Anime, manga, music and software (no login).",
    ru: "Аниме, манга, музыка и программы (без входа).",
  },
  author: "piTorrent",
  hosts: ["nyaa.si"],
  categories: ["anime", "music", "books", "movies", "tv", "software", "games"],
  settings: [],

  search: function (ctx, query, opts) {
    var CATS = { anime: "1_0", music: "2_0", books: "3_0", movies: "4_0", tv: "4_0", software: "6_1", games: "6_2" };
    var base = "https://nyaa.si";
    var out = [];
    // These sites turn away clients that do not look like a browser.
    var headers = { "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0" };
    for (var page = 1; page <= 3; page++) {
      var url = base + "/?" + new URLSearchParams({ f: 0, c: (opts.category && CATS[opts.category]) || "0_0", q: query, s: "seeders", o: "desc", p: page }).toString();
      var r = ctx.http.get(url, { headers: headers });
      if (!r.ok) {
        if (page > 1) break;
        throw new Error("nyaa.si answered " + r.status);
      }
      var rows = ctx.html.parse(r.text).select("table.torrent-list tbody tr");
      rows.forEach(function (row) {
        var cells = row.select("td");
        if (cells.length < 8) return;
        var title = cells[1].select("a").filter(function (a) { return !/comments/.test(a.attr("class") || ""); })[0];
        if (!title) return;
        var magnet = cells[2].first("a[href^='magnet:']");
        var file = cells[2].first("a[href$='.torrent']");
        var ts = cells[4].attr("data-timestamp");
        var kind = cells[0].first("a");
        out.push({
          title: title.text(),
          size: ctx.util.sizeToBytes(cells[3].text()),
          seeders: ctx.util.number(cells[5].text()),
          leechers: ctx.util.number(cells[6].text()),
          date: ts ? new Date(Number(ts) * 1000).toISOString().slice(0, 10) : null,
          magnet: magnet ? magnet.attr("href") : null,
          torrentUrl: file ? base + file.attr("href") : null,
          pageUrl: base + title.attr("href"),
          category: kind ? kind.attr("title") : null,
        });
      });
      if (rows.length < 75) break;
    }
    return out;
  },
});
