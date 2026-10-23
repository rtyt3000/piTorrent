<p align="center">
  <img src="assets/app_icon_256.png" width="112" alt="piTorrent logo">
</p>

<h1 align="center">piTorrent</h1>

<p align="center">
  A lightweight desktop BitTorrent client in Rust with tracker search through JavaScript plugins.
</p>

<p align="center">
  <a href="https://github.com/rtyt3000/piTorrent/actions/workflows/release.yml"><img src="https://github.com/rtyt3000/piTorrent/actions/workflows/release.yml/badge.svg" alt="Release"></a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/Rust-2024%20edition-orange?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/download-12%20MB-informational" alt="Size">
</p>

<p align="center">
  <b>English</b> · <a href="README.ru.md">Русский</a>
</p>

![piTorrent screenshot](assets/screenshot.png)

The interface is built with [iced](https://iced.rs), the torrent engine is [librqbit](https://github.com/ikatson/rqbit). Windows is the main platform (file associations and autostart go through the registry), but the code builds on other systems too.

## Features

- Downloading and seeding: `.torrent` files and magnet links, a file list preview before adding, picking individual files. Deselected files aren't created on disk; the pieces they share with their neighbours go to a hidden `.parts` file (like qBittorrent).
- Download queue with limits on simultaneous downloads and seeds, and priority changes (to the top / up / down / to the bottom).
- Speed limits, port, DHT.
- Categories with their own download folders.
- Creating a `.torrent` from a file or a folder.
- Torrent details: files (as a collapsible tree), peers (with country flags by IP), a piece map.
- Tracker search through plugins (see [docs/plugins.md](docs/plugins.md), in Russian): Jackett / Prowlarr, Internet Archive, your own plugins.
- Tray, closing to the tray (hold the close button to quit), autostart, completion notifications, a single running instance.
- Registers as the handler of `.torrent` and `magnet:`.
- Dark and light themes, English and Russian interface.

## Building and running

You need Rust with edition 2024 support.

### Linux

To build you need a C compiler, `pkg-config`, and the dev packages of GTK 3 and libayatana-appindicator (for the tray), for example on Fedora:

```sh
sudo dnf install gcc pkgconf-pkg-config gtk3-devel libayatana-appindicator-gtk3-devel libxkbcommon-devel wayland-devel
```

On Debian/Ubuntu: `build-essential pkg-config libgtk-3-dev libayatana-appindicator3-dev libxkbcommon-dev libwayland-dev`.

A ready-made archive (`piTorrent-<version>-linux-x86_64.tar.gz`) is available in the releases. It needs `libgtk-3` and `libayatana-appindicator3` (Fedora usually has them already; on Ubuntu: `libgtk-3-0 libayatana-appindicator3-1`).

Good to know:

- The tray works through StatusNotifier/AppIndicator. GNOME needs the AppIndicator extension for it; without it the icon isn't visible, so turn off "Close to tray" there.
- Plugin passwords are stored in the Secret Service (gnome-keyring, KWallet, KeePassXC).
- The associations button creates `~/.local/share/applications/piTorrent.desktop` with an icon and calls `xdg-mime` (the `xdg-utils` package is required).
- The window is frameless; there are no rounded corners or shadow.

```sh
cargo run --release
cargo test
```

Environment variables:

| Variable | Purpose |
|---|---|
| `PITORRENT_PROFILE=name` | A separate data folder (`piTorrent-name`) and single-instance port, to run a second copy side by side |
| `PITORRENT_DEMO=1` | A fake engine with sample torrents, for working on the interface and taking screenshots without a network |

## Releases

Releases are built by GitHub Actions ([.github/workflows/release.yml](.github/workflows/release.yml)): just push a `vX.Y.Z` tag that matches `version` in `Cargo.toml`. Archives for Windows and Linux are built (`scripts/package.ps1`, `scripts/package.sh`) with `.sha256` files and attached to the release. Build and test checks on both systems are in `.github/workflows/ci.yml`.

## Data

Stored in `<config dir>/piTorrent/` (on Windows, `%APPDATA%\piTorrent`):

- `settings.json`: app settings;
- `categories.json`: categories and their assignment to torrents (by info hash);
- `queue.json`: the queue and the priority order;
- `plugins.json` and `plugins/`: settings and installed plugins.

Plugin passwords and cookies aren't in a file; they are kept in the system credential store.

## Documentation

The documents below are in Russian.

- [docs/architecture.md](docs/architecture.md): how the project is organised.
- [docs/plugins.md](docs/plugins.md): how to write search plugins.
- [AGENTS.md](AGENTS.md): notes for AI agents and contributors.

The code is distributed under the [MIT](LICENSE) license. The icons in `assets/icons` are distributed under their own licenses (see the `LICENSE*` files in that folder). The font is Geist.

Peer countries are detected offline with the [IP to Country Lite](https://db-ip.com) database by DB-IP (CC BY 4.0); flags are from [flagcdn.com](https://flagcdn.com). The data in `assets/geo/` is generated by `tools/gen_geo.py`.
