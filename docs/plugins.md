# Плагины поиска

Плагин — один `.js`-файл, который вызывает `definePlugin({...})`. Выполняется в изолированном QuickJS, **синхронно**: `ctx.http.get(...)` сразу возвращает ответ. Установка — в «Настройки → Трекеры и плагины».

Встроенные:

| Файл | Источник |
|---|---|
| `torznab.js` | Jackett / Prowlarr (нужен адрес и ключ) |
| `internet-archive.js` | Internet Archive |
| `piratebay.js` | The Pirate Bay (JSON API apibay.org) |
| `torrents-csv.js` | torrents-csv.com |
| `limetorrents.js` | LimeTorrents |
| `bitsearch.js` | Bitsearch (бывший SolidTorrents) |
| `nyaa.js` | Nyaa.si |
| `rutor.js` | Rutor (открытый, без входа) |
| `example.js` | шаблон, показывающий весь API — удобно скопировать и изменить `id` |

Плагины без обязательных настроек включены сразу. Пять из них (кроме Torznab, Internet Archive и шаблона) — порты официальных поисковых плагинов qBittorrent; EZTV, TorLock и 1337x не перенесены, так как закрыты Cloudflare, а TorrentProject не работает.

Сайты меняют вёрстку и зеркала. Проверить плагины на реальных сайтах: `cargo test live_bundled_searches -- --ignored --nocapture`. Если сайту нужен «браузерный» клиент, передайте заголовок `User-Agent` через `ctx.http.get(url, { headers: {...} })`.

## Описание

```js
definePlugin({
  id: "my-tracker",            // уникальный идентификатор
  name: "My tracker",
  version: "1.0.0",
  description: { en: "...", ru: "..." },   // или просто строка
  author: "me",
  icon: "data:image/png;base64,...",   // необязательно: PNG до 64 КБ, лучше 32×32
  hosts: ["example.org", "$setting:baseUrl"],
  categories: ["movies", "music"],
  settings: [ /* см. ниже */ ],
  check:   function (ctx) { ... },                 // необязательно
  search:  function (ctx, query, opts) { ... },    // обязательно
  resolve: function (ctx, result) { ... },         // необязательно
});
```

Любой текст (`name`, `description`, `label`, `help`) может быть строкой или объектом `{ en, ru }` — берётся язык интерфейса, затем английский.

### hosts

Единственные сайты, к которым плагин может обращаться. `"example.org"` разрешает и поддомены (`api.example.org`). `"$setting:baseUrl"` — хост из URL-настройки `baseUrl`. Редиректы проверяются теми же правилами.

### settings

Поля `{ key, label, type, default, required, help, options }`. Типы: `text`, `password`, `cookie`, `url`, `number`, `bool`, `select` (с `options: [{ value, label }]`). Все значения приходят в `ctx.settings` строками. Значения `password` и `cookie` хранятся в системном хранилище учётных данных, а не в `plugins.json`.

## Методы

### `search(ctx, query, opts)`

`opts` — `{ category, limit }`. Возвращает массив результатов:

| Поле | Тип | |
|---|---|---|
| `title` | string | обязательно |
| `magnet` | string | magnet-ссылка |
| `torrentUrl` | string | прямая ссылка на `.torrent` |
| `pageUrl` | string | страница раздачи |
| `size` | число, байты | |
| `seeders`, `leechers` | число | |
| `date` | string | |
| `category` | string | |

Нужен хотя бы один из `magnet`, `torrentUrl`, `pageUrl`. Берётся не более 200 результатов; время поиска ограничено 45 секундами.

### `resolve(ctx, result)`

Для результатов, у которых есть только страница. Возвращает `{ magnet }`, `{ torrentUrl }` или `{ torrentBase64 }` (если плагин сам скачал `.torrent`, например из-за особых заголовков).

### `check(ctx)`

Вызывается кнопкой «Проверить». Вернуть строку с сообщением или бросить `Error`.

## API `ctx`

- `ctx.settings` — значения настроек.
- `ctx.log(...)` — вывод в stderr приложения (`[plugin <id>] info: ...`). Есть и `console.log/info/warn/error/debug`.
- `ctx.http`:
  - `get(url, opts)`, `post(url, form, opts)`, `postJson(url, json, opts)`, `request({ url, method, ... })`;
  - ответ содержит `text` и метод `json()`; ошибки выбрасываются как исключения;
  - cookie хранятся отдельно для каждого плагина; таймаут по умолчанию 15 с (максимум 30), размер ответа до 8 МБ.
- `ctx.html.parse(text)` → документ с `select(css)` и `first(css)`; элемент имеет `text()`, `html()`, `attr(name)`, `select(css)`, `first(css)`.
- `ctx.util`:
  - `sizeToBytes("1.5 GB")` (понимает KB/KiB/…, а также «МБ», «ГБ»);
  - `number("1 234")` → `1234`;
  - `magnet(infoHash, name, trackers)`.
- `ctx.state.get(key)` / `ctx.state.set(key, value)` — небольшое хранилище между вызовами (например, признак входа).

Также доступен `URLSearchParams`.

## Ограничения песочницы

Память 64 МБ, стек 1 МБ, доступ в сеть только к `hosts`, нет файловой системы и асинхронности. Поиск можно отменить — тогда HTTP-вызовы прерываются.
