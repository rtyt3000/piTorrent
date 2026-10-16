// The Pirate Bay through its public JSON API (apibay.org). Port of qBittorrent's piratebay plugin.
definePlugin({
  id: "piratebay",
  name: "The Pirate Bay",
  version: "1.0.0",
  description: {
    en: "Public torrents from The Pirate Bay (no login).",
    ru: "Открытые раздачи The Pirate Bay (без входа).",
  },
  author: "piTorrent",
  hosts: ["apibay.org", "thepiratebay.org"],
  categories: ["music", "movies", "games", "software"],
  settings: [],

  search: function (ctx, query, opts) {
    var CATS = { music: "100", movies: "200", software: "300", games: "400" };
    var TRACKERS = [
      "udp://tracker.opentrackr.org:1337/announce",
      "udp://tracker.openbittorrent.com:6969/announce",
      "udp://open.stealth.si:80/announce",
      "udp://exodus.desync.com:6969/announce",
      "udp://tracker.torrent.eu.org:451/announce",
      "udp://tracker.tiny-vps.com:6969/announce",
    ];
    var params = { q: query };
    if (opts.category && CATS[opts.category]) params.cat = CATS[opts.category];
    var r = ctx.http.get("https://apibay.org/q.php?" + new URLSearchParams(params).toString());
    if (!r.ok) throw new Error("apibay.org answered " + r.status);
    var out = [];
    r.json().forEach(function (x) {
      // An empty result is a single row with a zero hash.
      if (/^0+$/.test(x.info_hash)) return;
      out.push({
        title: x.name,
        size: Number(x.size),
        seeders: Number(x.seeders),
        leechers: Number(x.leechers),
        date: new Date(Number(x.added) * 1000).toISOString().slice(0, 10),
        magnet: ctx.util.magnet(x.info_hash, x.name, TRACKERS),
        pageUrl: "https://thepiratebay.org/description.php?id=" + x.id,
      });
    });
    return out;
  },
});
