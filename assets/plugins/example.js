 // A template that shows every part of the plugin API. Texts can be plain strings or
// { en: "...", ru: "..." } objects: the one for the interface language is shown.
// Copy it, change `id` and the code, and
// install the copy from Settings / Trackers and plugins.
//
//  * A plugin is a plain script that calls definePlugin({...}) once.
//  * Everything is synchronous: ctx.http.get(...) returns the response right away.
//  * `hosts` lists the only sites the plugin may talk to ("example.org" also allows
//    "api.example.org"). "$setting:baseUrl" means "the host of the setting named baseUrl".
//  * Settings with type "password" or "cookie" are stored in the system credential store.
definePlugin({
  id: "example",
  name: "Example (template)",
  version: "1.0.0",
  description: {
    en: "Shows the plugin API with fake results. Enter any API key to try it.",
    ru: "Показывает API плагинов на выдуманных результатах. Введите любой ключ API, чтобы попробовать.",
  },
  author: "piTorrent",
  hosts: [],
  categories: ["movies", "music"],
  settings: [
    { key: "apiKey", label: { en: "API key", ru: "Ключ API" }, type: "password", required: true, help: { en: "Any text works here.", ru: "Подойдёт любой текст." } },
    { key: "username", label: { en: "Login", ru: "Логин" }, type: "text" },
    { key: "password", label: { en: "Password", ru: "Пароль" }, type: "password" },
    { key: "cookie", label: "Cookie", type: "cookie", help: { en: "For example: uid=123; pass=abc", ru: "Например: uid=123; pass=abc" } },
    { key: "baseUrl", label: { en: "Site address", ru: "Адрес сайта" }, type: "url", default: "https://example.org" },
    { key: "pages", label: { en: "Pages to load", ru: "Сколько страниц загружать" }, type: "number", default: "1" },
    { key: "freeOnly", label: { en: "Only free-leech", ru: "Только фрилич" }, type: "bool", default: "false" },
    {
      key: "quality",
      label: { en: "Quality", ru: "Качество" },
      type: "select",
      default: "any",
      options: [
        { value: "any", label: { en: "Any", ru: "Любое" } },
        { value: "hd", label: { en: "HD only", ru: "Только HD" } },
      ],
    },
  ],

  // Optional: called by the "Test" button. Throw an Error to show a message.
  check: function (ctx) {
    if (!ctx.settings.apiKey) throw new Error("Enter the API key first");
    return "The settings look fine";
  },

  // Called for every search. Return an array of results.
  // Each result needs a title and at least one of: magnet, torrentUrl, pageUrl.
  search: function (ctx, query, opts) {
    ctx.log("searching for", query, "in", opts.category || "all categories");
    // Real plugins fetch a page and read it:
    //   var page = ctx.http.get(ctx.settings.baseUrl + "/search", { query: { q: query } });
    //   return page.html... ctx.html.parse(page.text).select("tr.result").map(function (row) { ... });
    return [
      {
        title: query + " (example result 1)",
        size: ctx.util.sizeToBytes("1.4 GB"),
        seeders: 42,
        leechers: 3,
        date: "2025-01-15",
        magnet: ctx.util.magnet("0123456789abcdef0123456789abcdef01234567", query),
        pageUrl: "https://example.org/torrent/1",
        category: opts.category || "movies",
      },
      {
        title: query + " (example result 2, needs resolve)",
        size: ctx.util.sizeToBytes("700 MiB"),
        seeders: 7,
        leechers: 0,
        date: "2025-01-10",
        pageUrl: "https://example.org/torrent/2",
        category: opts.category || "movies",
      },
    ];
  },

  // Optional: turns a result that only has a page into something that can be added.
  resolve: function (ctx, result) {
    return { magnet: ctx.util.magnet("fedcba9876543210fedcba9876543210fedcba98", result.title) };
  },
});
