#!/bin/sh
# Builds the release binary and packs it into dist/piTorrent-<version>-linux-<arch>.tar.gz
set -eu
cd "$(dirname "$0")/.."

cargo build --release

version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -n 1 | tr -d '\r')
name="piTorrent-$version-linux-$(uname -m)"
stage="dist/$name"

rm -rf "$stage"
mkdir -p "$stage"
cp "${CARGO_TARGET_DIR:-target}/release/piTorrent" README.md LICENSE "$stage/"
cp assets/icons/LICENSE*.txt "$stage/" 2>/dev/null || true

tar -C dist -czf "dist/$name.tar.gz" "$name"
ls -l "dist/$name.tar.gz"
