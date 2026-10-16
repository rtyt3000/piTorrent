// torrents-csv.com: an open database of torrents scraped from the DHT. Port of qBittorrent's
// torrentscsv plugin.
definePlugin({
  id: "torrents-csv",
  name: "torrents-csv",
  version: "1.0.0",
  description: {
    en: "Open torrent database built from DHT crawls (no login).",
    ru: "Открытая база торрентов, собранная из DHT (без входа).",
  },
  author: "piTorrent",
  hosts: ["torrents-csv.com"],
  categories: [],
  settings: [],

  search: function (ctx, query) {
    var TRACKERS = [
      "udp://tracker.opentrackr.org:1337/announce",
      "udp://tracker.openbittorrent.com:6969/announce",
      "udp://open.stealth.si:80/announce",
      "udp://exodus.desync.com:6969/announce",
      "udp://tracker.torrent.eu.org:451/announce",
    ];
    var r = ctx.http.get("https://torrents-csv.com/service/search?" + new URLSearchParams({ size: 100, q: query }).toString());
    if (!r.ok) throw new Error("torrents-csv.com answered " + r.status);
    return (r.json().torrents || []).map(function (x) {
      return {
        title: x.name,
        size: x.size_bytes,
        seeders: x.seeders,
        leechers: x.leechers,
        date: new Date(x.created_unix * 1000).toISOString().slice(0, 10),
        magnet: ctx.util.magnet(x.infohash, x.name, TRACKERS),
        pageUrl: "https://torrents-csv.com/#/search/torrent/" + encodeURIComponent(query) + "/1",
      };
    });
  },
});
