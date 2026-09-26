# ForgeConvert

Local-first, privacy-preserving desktop utility for image/PDF conversion, batch processing, optimization, and inspection. Files never leave your machine — no uploads, no accounts, no telemetry, no AI in the conversion pipeline.

Stack: **Tauri 2 + Rust core + Vue 3 + TypeScript + Vite + SQLite** (history/settings only; files stay files).

## Features

- **Convert**: PNG ↔ JPEG ↔ WebP (+ BMP/TIFF) with structured errors, never ad-hoc strings
- **Resize**: `--width/--height` exact, `--fit WxH` (aspect kept), `--fill WxH` (cover + center-crop), filters (Lanczos3/CatmullRom/Gaussian/Nearest), `--upscale` guard
- **Compression**: PNG level 0–9, JPEG/WebP quality 1–100, WebP lossless
- **Optimize presets**: `web`, `hq-jpeg`, `small-jpeg`, `lossless-png`, `webp` (config, not branches)
- **PDF**: images → PDF (A4/Letter, portrait/landscape, fit/fill); PDF → image stubbed as honest `Unsupported` until a renderer qualifies
- **Batch**: bounded worker pool, per-file + aggregate progress, exact ok/fail/skip counts
- **History**: SQLite-backed `history` command (metadata only)
- **Desktop UI**: Tauri shell + Vue UI (Convert/History tabs, preset chips, clickable reveal-in-folder results)

## Quick start

```powershell
cd dev
cargo build -p forgeconvert
./target/debug/forgeconvert.exe --help
```

```powershell
./target/debug/forgeconvert.exe info photo.png
./target/debug/forgeconvert.exe convert photo.png --to webp --quality 80
./target/debug/forgeconvert.exe convert photo.png --to webp --width 800 --height 600
./target/debug/forgeconvert.exe convert photo.png --to webp --fit 800x600 --filter lanczos3
./target/debug/forgeconvert.exe convert scan.jpg --to png --png-level 9
./target/debug/forgeconvert.exe batch ./images --to webp --jobs 4
./target/debug/forgeconvert.exe pdf a.png b.jpg --output doc.pdf
./target/debug/forgeconvert.exe optimize logo.png --preset webp
./target/debug/forgeconvert.exe history --limit 10
```

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
pnpm install --dir apps/desktop
pnpm --dir apps/desktop exec vue-tsc --noEmit
pnpm --dir apps/desktop exec vite build
pnpm --dir apps/desktop exec tauri dev
```

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

## Layout (repo root is `dev/`)

| Path | What |
|---|---|
| `product.md` | Product + master agent spec (authoritative) |
| `docs/PLAN.md` | Phased build plan 0→10 + dependency justifications |
| `docs/adr/` | Architecture decision records 001–012 |
| `docs/architecture/` | Layer + pipeline docs |
| `PROGRESS.md` | Step-by-step dev record (every step → commit) |
| `crates/forge-{core,engine,image,pdf,store}/` | Rust workspace members |
| `apps/cli/` | `forgeconvert` binary |
| `apps/desktop/` | Tauri 2 + Vue 3 frontend |
| `tests/{integration,fixtures,golden}/` | Round-trips, fixtures, property tests |
| `yma/` | Personal local notes (git-ignored, never pushed) |
