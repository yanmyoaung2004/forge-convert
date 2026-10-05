# ForgeConvert

![version](https://img.shields.io/github/v/release/yanmyoaung2004/forge-convert) ![ci](https://github.com/yanmyoaung2004/forge-convert/actions/workflows/ci.yml/badge.svg) ![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)

Local-first, privacy-preserving desktop utility for image/PDF conversion, batch processing, optimization, and inspection. Files never leave your machine — no uploads, no accounts, no telemetry, no AI in the conversion pipeline.

Stack: **Tauri 2 + Rust core + Vue 3 + TypeScript + Vite + SQLite** (history/settings only; files stay files).

## Features

- **Convert**: PNG ↔ JPEG ↔ WebP (+ BMP/TIFF) with structured errors, never ad-hoc strings
- **Resize**: `--width/--height` exact, `--fit WxH` (aspect kept), `--fill WxH` (cover + center-crop), filters (Lanczos3/CatmullRom/Gaussian/Nearest), `--upscale` guard
- **Compression**: PNG level 0–9, JPEG/WebP quality 1–100, WebP lossless; EXIF auto-rotate on decode
- **Optimize presets**: `web`, `hq-jpeg`, `small-jpeg`, `lossless-png`, `webp` (config, not branches) with before/after + % saved report

**PDF toolkit**
- **Write**: images → PDF (A4/Letter, portrait/landscape, fit/fill); PDF → images via hayro (pure-Rust CPU rasterizer, `render doc.pdf --format png --dpi 200`)
- **Split**: `pdf-split doc.pdf --pages 2-5` keeps pages 2–5 (`{stem}-split.pdf` default, `1,3,5-7` grammar)
- **Word**: `pdf-to-docx doc.pdf [--pages 1-3]` exports text to .docx (text-only, page breaks; scanned pages get a marker, never an error)
- **Merge**: `pdf-merge a.pdf b.pdf [--output merged.pdf]` concatenates pages in order
- **Compress**: `pdf-compress doc.pdf [--level light|balanced]` — prune orphans, or + recompress streams

**Icons & QR**
- **Favicon**: `favicon in.png [--sizes 16,32,48,180,192,512]` — `favicon.ico` (16/32/48) + sized PNGs + copy-paste `<link rel=icon>` snippet
- **QR encode**: `qr "text" [--ec L|M|Q|H] [--format png|svg] [--no-quiet-zone] [--dark #..] [--light #..] [--logo mark.png]` — EC 7–30%, PNG + vector SVG, custom colors, center logo (PNG, ≤20%, EC-H recommended)
- **QR decode**: `qr-decode img.png` reads QR text from any image (`rqrr`, pure-Rust)
- **QR batch**: `qr-batch list.txt [out-dir]` — one QR per line (`#`/blank skip, BOM+CRLF tolerant, `qr-{001..}`, never abort-all, `N ok / M failed / K skipped`)
- **QR payloads**: WiFi (SSID/pass/security/hidden, spec escaping), URL (scheme check), mailto, SMS, vCard 3.0 (CRLF folding) — desktop Tools tab builders + raw-text CLI

**Text/data toolbox** (desktop Tools tab, `apps/desktop/src/tools/texttools.ts` — pure functions, bun-smoked)
- JSON pretty/minify/validate, base64 encode/decode, UUID v4, ms-epoch ↔ ISO, JWT decode (no-verify, labelled), regex tester
- SHA-256 file hash (streamed in Rust, 512 MiB cap; `hash file` in CLI)

**History**: SQLite-backed `history` command (metadata only; best-effort record on convert/optimize/split/merge/compress/render/qr)

**Desktop UI**: Tauri shell + Vue UI (Convert/PDF/Diagram/Tools/History tabs, preset chips, drag-drop, cancel + progress bar, per-file sizes + savings, clickable reveal-in-folder results)

**SVG Diagram editor** (`apps/desktop/src/diagram/`, 28 registry kinds in `elements.ts`)
- Shapes, developer blocks (input/output/process/database/server/api/client/cloud/queue/user/document/storage), UML (actor/use-case/class/package/note), ERD (table/column/key/FK)
- Connections (straight/orthogonal/curved, arrows/dash), undo/redo snapshots, auto-layout, 7 templates, save/load `.fdiag.json`
- Bidirectional SVG (paste SVG → editable blocks), Export SVG + PNG/PNG 2x (canvas rasterize via `export_png_file`)

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
./target/debug/forgeconvert.exe render doc.pdf --format png --dpi 200
./target/debug/forgeconvert.exe pdf-split doc.pdf --pages 2-5
./target/debug/forgeconvert.exe pdf-to-docx doc.pdf --pages 1-3
./target/debug/forgeconvert.exe pdf-merge a.pdf b.pdf --output merged.pdf
./target/debug/forgeconvert.exe pdf-compress doc.pdf --level balanced
./target/debug/forgeconvert.exe favicon logo.png --sizes 16,32,48,180,192,512
./target/debug/forgeconvert.exe optimize logo.png --preset webp
./target/debug/forgeconvert.exe qr "https://example.com" --ec H --format svg
./target/debug/forgeconvert.exe qr "https://example.com" --dark "#1a2b3c" --light "#f0e6d2" --logo mark.png
./target/debug/forgeconvert.exe qr-decode qr.png
./target/debug/forgeconvert.exe qr-batch list.txt qr-batch
./target/debug/forgeconvert.exe history --limit 10
```

Exit codes: `2` bad config · `3` bad format · `4` bad file · `5` output exists · `6` permission · `7` disk full · `8` unsupported · `130` cancelled.

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

## [Download — v0.7.0](https://github.com/yanmyoaung2004/forge-convert/releases/tag/v0.7.0)

| Asset | What | How to run |
| [forgeconvert-windows-x86_64.exe](https://github.com/yanmyoaung2004/forge-convert/releases/download/v0.7.0/forgeconvert-windows-x86_64.exe) | CLI, Windows x64 | `forgeconvert-windows-x86_64.exe --help` |
| [forgeconvert-linux-x86_64](https://github.com/yanmyoaung2004/forge-convert/releases/download/v0.7.0/forgeconvert-linux-x86_64) | CLI, Linux x64 | `chmod +x forgeconvert-linux-x86_64 && ./forgeconvert-linux-x86_64 --help` |
| [forgeconvert-macos-aarch64](https://github.com/yanmyoaung2004/forge-convert/releases/download/v0.7.0/forgeconvert-macos-aarch64) | CLI, macOS Apple Silicon | `chmod +x forgeconvert-macos-aarch64 && ./forgeconvert-macos-aarch64 --help` |
| `ForgeConvert_0.7.0_x64_en-US.msi` | Desktop installer (styled QR + PDF render + diagram PNG) | Double-click (per-machine install) |
| `ForgeConvert_0.7.0_x64-setup.exe` | Desktop installer (styled QR + PDF render + diagram PNG) | Double-click (NSIS wizard) |

> v0.7.0: styled QR (`qr --dark/--light/--logo`, `QrStyle` + center-mark composite) + PDF render to images (hayro 0.6, live `render` CLI + desktop row) + diagram PNG export (canvas rasterize 1x/2x via `export_png_file`).

> v0.6.0: `qr-batch list.txt [out-dir]` (one QR per line, `#`/blank skip, BOM+CRLF tolerant, `qr-{001..}`, per-line failures, `N ok / M failed / K skipped`) + history for `qr`/`qr-decode`/`qr-batch` + desktop Reveal button + `--no-quiet-zone` flag + QR panel quiet-zone checkbox (default on).

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
| `product.md` (yma/) | Product + master agent spec (authoritative) |
| `docs/PLAN.md` | Phased build plan 0→10 + dependency justifications |
| `docs/PLAN-svg-diagram.md` | Diagram editor plan |
| `docs/adr/` (001–014) | Architecture decision records |
| `docs/architecture/layers.md` | Layer + pipeline docs |
| `yma/` | Plans + dev record (`PROGRESS.md`), git-ignored work notes |
| `crates/forge-{core,engine,image,pdf,store}/` | Rust workspace members |
| `apps/cli/` | `forgeconvert` binary |
| `apps/desktop/` (Tauri 2 + Vue 3) | `src-tauri/` commands, `src/` UI incl. `diagram/`, `tools/` |
| `tests/fixtures/`, `tests/golden/` | Fixture docs, golden outputs |
