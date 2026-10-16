# Builds the release binary and packs it into dist/piTorrent-<version>-windows-x64.zip
$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "build failed" }

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"(.+)"').Matches[0].Groups[1].Value
$name = "piTorrent-$version-windows-x64"
$stage = "dist/$name"
Remove-Item -Recurse -Force dist -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null

Copy-Item target/release/piTorrent.exe $stage
Copy-Item README.md $stage
Copy-Item assets/icons/LICENSE*.txt $stage -ErrorAction SilentlyContinue

Compress-Archive -Path $stage -DestinationPath "dist/$name.zip"
$size = [math]::Round((Get-Item "dist/$name.zip").Length / 1MB, 1)
Write-Host "dist/$name.zip ($size MB)"
