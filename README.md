# ForgeConvert

![version](https://img.shields.io/github/v/release/yanmyoaung2004/forge-convert) ![ci](https://github.com/yanmyoaung2004/forge-convert/actions/workflows/ci.yml/badge.svg) ![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)

Local-first, privacy-preserving desktop utility for image/PDF conversion, batch processing, optimization, and inspection. Files never leave your machine — no uploads, no accounts, no telemetry, no AI in the conversion pipeline.

Stack: **Tauri 2 + Rust core + Vue 3 + TypeScript + Vite + SQLite** (history/settings only; files stay files).

## Features

- **Convert**: PNG ↔ JPEG ↔ WebP (+ BMP/TIFF) with structured errors, never ad-hoc strings
- **Resize**: `--width/--height` exact, `--fit WxH` (aspect kept), `--fill WxH` (cover + center-crop), filters (Lanczos3/CatmullRom/Gaussian/Nearest), `--upscale` guard
- **Compression**: PNG level 0–9, JPEG/WebP quality 1–100, WebP lossless
- **Optimize presets**: `web`, `hq-jpeg`, `small-jpeg`, `lossless-png`, `webp` (config, not branches)
- **PDF**: images → PDF (A4/Letter, portrait/landscape, fit/fill); PDF → image stubbed as honest `Unsupported` until a renderer qualifies
- **PDF split**: `pdf-split doc.pdf --pages 2-5` keeps pages 2–5 (`{stem}-split.pdf` default, same `1,3,5-7` grammar as `render --pages`)
- **PDF → Word**: `pdf-to-docx doc.pdf [--pages 1-3]` exports text to .docx (text-only, page breaks; scanned pages get a marker, never an error)
- **History**: SQLite-backed `history` command (metadata only)
- **QR encode**: `qr "text" [--ec L|M|Q|H] [--format png|svg] [--no-quiet-zone]` — EC 7–30% recovery, PNG + vector SVG, quiet-zone toggle
- **QR decode**: `qr-decode img.png` reads QR text from any image (`rqrr`, pure-Rust)
- **QR batch**: `qr-batch list.txt [out-dir]` — one QR per line (`#`/blank skip, BOM+CRLF tolerant, `qr-{001..}`, never abort-all)
- **QR payloads**: WiFi (SSID/pass/security/hidden, spec escaping), URL (scheme check), mailto, SMS, vCard 3.0 (CRLF folding) — desktop Tools tab builders + raw-text CLI
The repo root is the checkout root (`forge-convert/`); all commands run from there.

```powershell
cargo build -p forgeconvert
./target/debug/forgeconvert.exe --help
./target/debug/forgeconvert.exe info photo.png
./target/debug/forgeconvert.exe convert photo.png --to webp --quality 80
./target/debug/forgeconvert.exe convert photo.png --to webp --width 800 --height 600
./target/debug/forgeconvert.exe convert photo.png --to webp --fit 800x600 --filter lanczos3
./target/debug/forgeconvert.exe convert scan.jpg --to png --png-level 9
./target/debug/forgeconvert.exe batch ./images --to webp --jobs 4
./target/debug/forgeconvert.exe pdf a.png b.jpg --output doc.pdf
./target/debug/forgeconvert.exe pdf-split doc.pdf --pages 2-5
./target/debug/forgeconvert.exe pdf-to-docx doc.pdf --pages 1-3
./target/debug/forgeconvert.exe optimize logo.png --preset webp
./target/debug/forgeconvert.exe qr "https://example.com" --ec H --format svg
./target/debug/forgeconvert.exe qr-decode qr.png
./target/debug/forgeconvert.exe qr-batch list.txt qr-batch
./target/debug/forgeconvert.exe history --limit 10

Exit codes: `2` bad config · `3` bad format · `4` bad file · `5` output exists · `6` permission · `7` disk full · `8` unsupported (e.g. PDF render) · `130` cancelled.

## Tests & gates

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Benches: `cargo bench -p forge-engine --bench throughput -- --quick`.

Desktop UI:

```powershell
pnpm install --dir dev/apps/desktop
pnpm --dir dev/apps/desktop exec vue-tsc --noEmit
pnpm --dir dev/apps/desktop exec vite build
pnpm --dir dev/apps/desktop exec tauri dev
```

## [Download — v0.6.0](https://github.com/yanmyoaung2004/forge-convert/releases/tag/v0.6.0)

| Asset | What | How to run |
|---|---|---|
| [forgeconvert-windows-x86_64.exe](https://github.com/yanmyoaung2004/forge-convert/releases/download/v0.6.0/forgeconvert-windows-x86_64.exe) | CLI, Windows x64 | `forgeconvert-windows-x86_64.exe --help` |
| [forgeconvert-linux-x86_64](https://github.com/yanmyoaung2004/forge-convert/releases/download/v0.6.0/forgeconvert-linux-x86_64) | CLI, Linux x64 | `chmod +x forgeconvert-linux-x86_64 && ./forgeconvert-linux-x86_64 --help` |
| [forgeconvert-macos-aarch64](https://github.com/yanmyoaung2004/forge-convert/releases/download/v0.6.0/forgeconvert-macos-aarch64) | CLI, macOS Apple Silicon | `chmod +x forgeconvert-macos-aarch64 && ./forgeconvert-macos-aarch64 --help` |
| `ForgeConvert_0.6.0_x64_en-US.msi` | Desktop installer (QR batch + quiet zone + reveal) | Double-click (per-machine install) |
| `ForgeConvert_0.6.0_x64-setup.exe` | Desktop installer (QR batch + quiet zone + reveal) | Double-click (NSIS wizard) |

> v0.6.0: `qr-batch list.txt [out-dir]` (one QR per line, `#`/blank skip, BOM+CRLF tolerant, `qr-{001..}`, per-line failures, `N ok / M failed / K skipped`) + history for `qr`/`qr-decode`/`qr-batch` + desktop Reveal button + `--no-quiet-zone` flag + QR panel quiet-zone checkbox (default on).

> v0.5.0: QR encode upgrades (`qr --ec L|M|Q|H --format png|svg`, shared `encode_qr` in `forge-image`) + QR decode (`qr-decode img.png` via `rqrr`; first-grid-wins, clean errors) + **QR payload builders** (WiFi/URL/mailto/SMS/vCard 3.0 in Tools tab, 29-check bun smoke). Desktop QR panel gains EC dropdown, PNG/SVG toggle, payload tabs, decode button.

## Architecture

Clean layered + hexagonal (ports & adapters), dependencies point inward:

```text
apps/* (CLI, Tauri UI — thin: parse → call engine → render)
  → forge-engine (orchestrator, job manager, batch pool, naming, presets)
    → forge-core (domain + ports, zero internal deps)
  adapters → ports: forge-image, forge-pdf, forge-store
```

- Canonical-image rule: every decoder → `ImageBuffer` → transform pipeline → encoder. No N×M converters.
- Writes: temp → flush/sync → validate → atomic rename. Collisions: Rename-auto default, else `OutputExists`.

## Layout

| Path | What |
|---|---|
| `dev/product.md` | Product + master agent spec (authoritative) |
| `dev/docs/PLAN.md` | Phased build plan 0→10 + dependency justifications |
| `dev/docs/adr/` | Architecture decision records 001–012 |
| `dev/docs/architecture/` | Layer + pipeline docs |
| `dev/PROGRESS.md` | Step-by-step dev record (every step → commit) |
| `dev/crates/forge-{core,engine,image,pdf,store}/` | Rust workspace members |
| `dev/apps/cli/` | `forgeconvert` binary |
| `dev/apps/desktop/` | Tauri 2 + Vue 3 frontend |
| `dev/tests/{integration,fixtures,golden}/` | Round-trips, fixtures, property tests |
| `dev/yma/` | Personal local notes (git-ignored, never pushed) |
