# AGENTS.md

Заметки для ИИ-агентов, работающих с репозиторием. Обзор для людей — в [README.md](README.md), детали — в [docs/](docs/).

## Команды

```sh
cargo build
cargo test            # в основном тесты плагинов (src/plugins/tests.rs)
cargo run             # PITORRENT_DEMO=1 — без сети, с фиктивными торрентами
PITORRENT_PROFILE=dev cargo run   # отдельные данные, можно держать рядом с основной копией
```

## Структура

- `src/main.rs` — приложение iced: `App`, `Msg`, `update`, подписки, точка входа.
- `src/view.rs`, `src/view_plugins.rs`, `src/widgets.rs`, `src/theme.rs`, `src/icons.rs` — интерфейс и стили.
- `src/engine/` — граница с торрент-библиотекой: трейт `Backend`, `Engine` (очередь поверх бэкенда), `rqbit.rs` (librqbit), `demo.rs` (фейк).
- `src/plugins/` — песочница JS-плагинов (QuickJS): `host.rs`, `http.rs`, `manager.rs`, `store.rs`, `tests.rs`.
- `src/search.rs` — параллельный поиск по включённым плагинам, кэш, сортировка.
- `src/settings.rs`, `src/categories.rs` — персистентные данные в JSON.
- Платформа: `assoc.rs`, `autostart.rs`, `single_instance.rs`, `tray.rs`, `notify.rs`, `platform.rs`, `app_icon.rs`.
- `src/i18n.rs` + `assets/locales/{en,ru}.json` — строки интерфейса.
- `assets/plugins/` — встроенные плагины и `prelude.js` (вшиваются через `include_str!`).

## Правила

- Любая новая строка интерфейса добавляется в **оба** файла `assets/locales/en.json` и `ru.json`; в коде — через `tr("key")` / `trf`.
- Код, зависящий от конкретной торрент-библиотеки, держать в `engine/rqbit.rs`. Логика, не зависящая от неё (очередь), — в `Engine`. Новый метод бэкенда нужно реализовать и в `demo.rs`.
- Плагины синхронные и изолированные: доступ в сеть только к хостам из `hosts`, лимиты по памяти/времени/размеру. Не ослаблять песочницу без явной причины. Секреты (`password`, `cookie`) — только в keyring, никогда в `plugins.json`.
- Новый встроенный плагин: положить в `assets/plugins/`, добавить в `BUNDLED` в `src/plugins/manager.rs`.
- Новое поле в `Settings` должно иметь значение по умолчанию (`#[serde(default)]` на структуре уже есть), чтобы старые `settings.json` читались.
- Файлы данных пишутся «best effort» (ошибки записи игнорируются), читаются с откатом на значения по умолчанию.
- Стиль: комментарии на английском, только там, где объясняют «почему»; без лишних абстракций.
- Приложение запускается без консоли в release (`windows_subsystem = "windows"`), поэтому отладочный вывод смотреть в debug-сборке.

## Что проверять перед завершением

`cargo build` и `cargo test` проходят; при правке интерфейса — запуск с `PITORRENT_DEMO=1`.
