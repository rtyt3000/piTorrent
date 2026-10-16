// Searches through your own Jackett or Prowlarr, so every tracker configured there (public or
// private, with its own login) is available here. piTorrent never talks to those trackers itself.
definePlugin({
  id: "torznab",
  name: "Jackett / Prowlarr",
  version: "1.0.0",
  description: {
    en: "Search all trackers configured in your Jackett or Prowlarr at once.",
    ru: "Поиск сразу по всем трекерам, настроенным в вашем Jackett или Prowlarr.",
  },
  author: "piTorrent",
  hosts: ["$setting:baseUrl"],
  categories: ["movies", "tv", "music", "games", "software", "books", "anime"],
  settings: [
    {
      key: "flavor",
      label: { en: "Program", ru: "Программа" },
      type: "select",
      default: "jackett",
      options: [
        { value: "jackett", label: "Jackett" },
        { value: "prowlarr", label: "Prowlarr" },
      ],
    },
    {
      key: "baseUrl",
      label: { en: "Address", ru: "Адрес" },
      type: "url",
      default: "http://127.0.0.1:9117",
      required: true,
      help: {
        en: "Jackett is usually http://127.0.0.1:9117, Prowlarr http://127.0.0.1:9696",
        ru: "Обычно Jackett: http://127.0.0.1:9117, Prowlarr: http://127.0.0.1:9696",
      },
    },
    {
      key: "apiKey",
      label: { en: "API key", ru: "Ключ API" },
      type: "password",
      required: true,
      help: {
        en: "Shown at the top of the Jackett page, or in Prowlarr under Settings / General.",
        ru: "Указан вверху страницы Jackett, а в Prowlarr — в Settings / General.",
      },
    },
    {
      key: "indexer",
      label: { en: "Indexer", ru: "Индексер" },
      type: "text",
      default: "all",
      help: {
        en: "Jackett only: \"all\" or the id of one indexer.",
        ru: "Только для Jackett: «all» или идентификатор одного индексера.",
      },
    },
  ],

  check: function (ctx) {
    var s = ctx.settings;
    var base = s.baseUrl.replace(/\/+$/, "");
    if (s.flavor === "prowlarr") {
      var list = ctx.http.get(base + "/api/v1/indexer", { headers: { "X-Api-Key": s.apiKey } });
      if (!list.ok) throw new Error("Prowlarr answered " + list.status + " (check the address and the API key)");
      return list.json().length + " indexers";
    }
    var r = ctx.http.get(base + "/api/v2.0/indexers", { query: { apikey: s.apiKey, configured: "true" } });
    if (!r.ok) throw new Error("Jackett answered " + r.status + " (check the address and the API key)");
    return r.json().length + " indexers";
  },

  search: function (ctx, query, opts) {
    var s = ctx.settings;
    var base = s.baseUrl.replace(/\/+$/, "");
    var CATS = { movies: 2000, tv: 5000, music: 3000, games: 1000, software: 4000, books: 7000, anime: 5070 };
    var cat = opts.category && CATS[opts.category];

    if (s.flavor === "prowlarr") {
      var q = { query: query, type: "search", limit: 100 };
      if (cat) q.categories = cat;
      var r = ctx.http.get(base + "/api/v1/search", { query: q, headers: { "X-Api-Key": s.apiKey } });
      if (!r.ok) throw new Error("Prowlarr answered " + r.status);
      return r.json().map(function (x) {
        return {
          title: x.title,
          size: x.size,
          seeders: x.seeders,
          leechers: x.leechers,
          date: x.publishDate ? String(x.publishDate).slice(0, 10) : null,
          magnet: x.magnetUrl,
          torrentUrl: x.downloadUrl,
          pageUrl: x.infoUrl,
          category: x.indexer,
        };
      });
    }

    var indexer = (s.indexer || "all").trim() || "all";
    var params = new URLSearchParams({ apikey: s.apiKey, Query: query });
    if (cat) params.append("Category[]", cat);
    var url = base + "/api/v2.0/indexers/" + encodeURIComponent(indexer) + "/results?" + params.toString();
    var res = ctx.http.get(url);
    if (!res.ok) throw new Error("Jackett answered " + res.status);
    return (res.json().Results || []).map(function (x) {
      return {
        title: x.Title,
        size: x.Size,
        seeders: x.Seeders,
        leechers: x.Peers != null && x.Seeders != null ? Math.max(0, x.Peers - x.Seeders) : null,
        date: x.PublishDate ? String(x.PublishDate).slice(0, 10) : null,
        magnet: x.MagnetUri,
        torrentUrl: x.Link,
        pageUrl: x.Details,
        category: x.Tracker,
      };
    });
  },
});
