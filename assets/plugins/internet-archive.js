// Free books, films, music and software from the Internet Archive (public API, no login).
definePlugin({
  id: "internet-archive",
  name: "Internet Archive",
  version: "1.0.0",
  description: {
    en: "Open archives: books, films, music and software.",
    ru: "Открытые архивы: книги, фильмы, музыка и программы.",
  },
  author: "piTorrent",
  hosts: ["archive.org"],
  categories: ["movies", "music", "books", "software"],
  settings: [],

  search: function (ctx, query, opts) {
    var TYPES = { movies: "movies", music: "audio", books: "texts", software: "software" };
    var kinds = opts.category && TYPES[opts.category] ? [TYPES[opts.category]] : ["movies", "audio", "texts", "software"];
    var q = "(" + query + ") AND mediatype:(" + kinds.join(" OR ") + ")";
    // The API wants repeated fl[] parameters, which a plain object cannot express, so the
    // URL is built by hand.
    var fields = ["identifier", "title", "item_size", "downloads", "publicdate", "mediatype"];
    var url =
      "https://archive.org/advancedsearch.php?" +
      new URLSearchParams({ q: q, rows: 50, output: "json", "sort[]": "downloads desc" }).toString() +
      fields.map(function (f) { return "&fl[]=" + f; }).join("");
    var r = ctx.http.get(url);
    if (!r.ok) throw new Error("archive.org answered " + r.status);
    var CATEGORY = { movies: "movies", audio: "music", texts: "books", software: "software" };
    return r.json().response.docs.map(function (d) {
      var id = d.identifier;
      return {
        title: d.title || id,
        size: d.item_size,
        seeders: null,
        leechers: null,
        date: d.publicdate ? String(d.publicdate).slice(0, 10) : null,
        torrentUrl: "https://archive.org/download/" + id + "/" + id + "_archive.torrent",
        pageUrl: "https://archive.org/details/" + id,
        category: CATEGORY[d.mediatype] || d.mediatype,
      };
    });
  },
});
