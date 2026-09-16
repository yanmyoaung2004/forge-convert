# ForgeConvert — Development Record (PROGRESS.md)

> Step-by-step build log. Every completed step links to its git commit.
> Spec source: `dev/product.md` (2731 lines). Architecture: clean / layered +
> hexagonal (ports & adapters) influence, per spec §§5–6 + Master Spec §2.
> Goal stated by owner: **working product at the end**.

## Git workflow

- One commit per task/step, messages prefixed `step-N:` (+ `plan:`, `docs:`, `feat:`, `test:`, `fix:`, `chore:` where clearer).
- `git log --oneline` is the source of truth for order; this file is the narrative.

## Step log

| # | Date (UTC) | Step | What was done | Commit |
|---|------------|------|---------------|--------|
| 0 | 2026-09-17 | Spec intake | Read `dev/product.md` fully (all 2731 lines: proposal §§1–48 + Master Spec §§1–36). Wrote `AGENTS.md` repo guidelines via 4 parallel scouts. | _pre-git_ |
| 1 | 2026-09-17 | Repo foundation | `git init`, `main` branch, `.gitignore` (Rust/Node/Tauri), this `PROGRESS.md` record. | `8f10ceb` |
| 2 | 2026-09-17 | Toolchain | Repaired interrupted rustup update (`rustup update stable` → `rustc 1.98.1`, `cargo 1.98.1`, host `x86_64-pc-windows-msvc`). Verified MSVC 14.44 + WinSDK 10.0.26100.0 + WebView2 153 → Tauri 2 unblocked. | _see log_ |
| 3 | 2026-09-17 | Stack research | 4 parallel scouts + direct index/raw verification (see Research facts below). Key: `image` WebP encode is lossless-only → need `webp` crate for lossy q80; `printpdf` for image→PDF; PDF render deferred; Tauri MSVC-gated but unblocked. | _see log_ |
| 4 | 2026-09-17 | Detailed plan + dev/ move | `docs/PLAN.md` (Phase 0→10, hexagonal layout, dep-justification table). Rewrote `AGENTS.md` for new root. Moved repo root into `dev/` (`.git/` + all tracked files live under `dev/`; parent holds only `dev/`). | _see log_ |
| 5 | 2026-09-17 | Scaffold (Phase 0) | Workspace `Cargo.toml` (members `apps/cli`, `crates/*`), `rust-toolchain.toml`, 5 lib crates + CLI bin (compiling stubs), ADRs 001–012, `docs/architecture/layers.md`, `tests/fixtures/README.md`, CI (`fmt`+`clippy -D warnings`+`test`). Gates: `cargo build/test/clippy` green (0 tests, skeleton). Shell note: Git-Bash can't exec cargo shims; use PowerShell `& \"$env:USERPROFILE\\.cargo\\bin\\cargo.exe\"`. | `da5d8bb` |
| 6 | 2026-09-17 | Phase 1 domain | `forge-core`: formats + magic-byte detection + capabilities, options (quality/resize/metadata/bg/collisions/page-range) + validation, jobs + validated transitions + naming (`logo.png→logo.webp`), presets, `CanonicalImage`, ports (decoder/encoder/transform/pdf/fs/history/clock/engine/cancel), `ForgeError` (+`InvalidTransition`,`Unsupported`). 24 unit tests. | `cb8f443` |
| 7 | 2026-09-17 | Phase 2 image codecs | `forge-image`: decode PNG/JPEG/WebP/BMP/TIFF; encode PNG (alpha-preserving) / JPEG (`new_with_quality`) / lossy WebP (`webp::Encoder::encode(q f32)`, ADR 012) / BMP+TIFF; RGBA→JPEG flatten over white; `FitWithinStep` (Lanczos3). Enabled `bmp,tiff,webp` image features. 6 roundtrip tests. | `835f1d7` |
| 8 | 2026-09-17 | Phase 3+4 engine | `forge-engine`: `Orchestrator::run` (§11 pipeline, cancel between stages, same-format reject, per-file failures), `StdFileSystem` (bounded reads, temp→sync→validate→atomic rename, structured IO errors), `resolve_output`/`apply_collision` (Rename-auto/Replace/Skip/Fail), `run_batch` (tokio semaphore + `spawn_blocking`, exact ok/fail/skip counts). 15 tests. | `02ad43f` |
| 9 | 2026-09-17 | Phase 5 PDF | `forge-pdf`: `ForgePdfWriter` (printpdf `images` feature, `RawImage{data_format:RGBA8/RGB8}` struct literal — verified vs vendored 0.12.8 sources, A4/Letter + portrait/landscape + fit/fill/none + margins + dpi, one page/image, `%PDF` magic), `StubPdfRenderer` → `Unsupported`. 4 tests. | `b326ebc` |
| 10 | 2026-09-17 | Phase 7 CLI | `forgeconvert` binary: `convert --to --quality --max-* --strip-metadata --on-collision`, `batch --jobs` (bounded pool), `pdf --page --landscape`, `render` (honest `Unsupported`, exit 8), `info`, `optimize --preset web` (before/after + % saved). Skip-aware `ConversionResult.skipped`; exits 2/3/4/5/6/7/8/130. Smoke-verified: PNG→WebP, PNG→JPG (flatten), BMP→PNG, optimize, multi-image PDF, batch 3ok/1fail, collision fail=5/skip=0. | `55aad9e` |
| 11+ | — | Remaining | Phase 6 Tauri UI → Phase 8 optimization extras (resize presets, metadata strip) → Phase 9 SQLite history → Phase 10 hardening/benches/packaging. | — |

## Environment facts (verified, not guessed)

- OS: Windows 11 x64. Disk: 44G free on D:.
- `node v24.14.0`, `bun 1.3.14`, `pnpm 12.4.1`, `git 2.42.0`, `curl 8.21.0` present.
- `winget`: NOT available. `code` CLI: not on PATH.
- MinGW `gcc 6.3.0` (32-bit `mingw32`, 2017): too old — NOT used (MSVC path chosen).
- `cargo`/`rustc`: repaired to `cargo 1.98.1` / `rustc 1.98.1 (host x86_64-pc-windows-msvc, LLVM 22.1.8)`. Shell note: Git-Bash shell cannot exec cargo shims (symlink); use `powershell.exe -NoProfile -Command "& \"$env:USERPROFILE\\.cargo\\bin\\cargo.exe\" …"`.
- **VS Build Tools 2022 present** (`C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools`, MSVC 14.44.35207 verified) + **WinSDK 10.0.26100.0** + **WebView2 153.0.4234.32** → default `x86_64-pc-windows-msvc` target; Tauri 2 prerequisites satisfied (Build Tools + SDK + WebView2 all present).

## Research facts (Step 3, verified 2026-09-17 — sources: crates.io sparse index + GitHub raw + official docs)

- **image 0.25.10** (`MIT OR Apache-2.0`, ed.2021, MSRV 1.88): default-formats = avif/bmp/dds/exr/ff/gif/hdr/ico/jpeg/png/pnm/qoi/tga/tiff/webp. JPEG via `zune-jpeg` (`JpegEncoder::new_with_quality(w, u8)` verified); PNG via `png` crate; BMP/TIFF pure-Rust. **WebP encode via `image-webp 0.2.4` is VP8L lossless-only** (`WebPEncoder::new` doc comment verified) → lossy q80 needs `webp` crate.
- **webp 0.3.1** (`MIT OR Apache-2.0`, jaredforth/webp): `Encoder::from_rgb/from_rgba/from_image`, `encode(quality: f32)`, `encode_lossless`, `encode_simple/advanced` (verified). Cost: `libwebp-sys 0.9.3` native C (needs C compiler — MSVC present, OK).
- **Optional later:** `mozjpeg 0.10.13` (native, better JPEG), `oxipng 10.2.1` (PNG opt), `kamadak-exif 0.6.1` (EXIF read, Phase 8). Not MVP.
- **printpdf 0.12.8** (MIT, 2026-09-05) pure-Rust image→PDF path (`PdfDocument::new` + `RawImage::decode_from_bytes` + `Op::UseXobject{XObjectTransform{scale_x/y, dpi…}}` + `PdfPage::new(Mm(210),Mm(297))`, per repo `examples/image.rs`). A4 = `Mm(210)×Mm(297)`; Letter = `Mm(215.9)×Mm(279.4)` (computed, no named const found). No fit/fill helper — caller computes scale.
- **lopdf 0.45.0** (MIT) pure-Rust low-level PDF; take directly only if printpdf API outgrown (it is printpdf's engine).
- **PDF render: DEFERRED.** `hayro 0.7.1` (Apache-2.0 OR MIT, pure-Rust, `forbid(unsafe)`, MSRV 1.92) experimental — gaps: encrypted PDFs, blending/isolation, perf. `pdfium-render 0.9.4` (crate MIT OR Apache-2.0; **Pdfium binary license UNVERIFIED**) needs shipped `pdfium.dll` from `bblanchon/pdfium-binaries`. REJECT `mupdf 0.8.0` (AGPL-3.0) and `poppler 0.6.0` (stale 2024, system-native). Default build: render API returns structured `Unsupported`.
- **Tauri 2: MSVC mandatory for supported builds** ([prerequisites](https://v2.tauri.app/start/prerequisites/), [installer](https://v2.tauri.app/distribute/windows-installer/)) — satisfied here. `tauri 2.11.5` stable (2.11.3–2.11.5 observed; 3.0.0-alpha exists — DO NOT use). `@tauri-apps/cli 2.11.4`, `@tauri-apps/api 2.11.1`, `plugin-dialog 2.7.3`, `plugin-fs 2.5.2` (frontend pure JS, no MSVC).
- **Infra (all pure-Rust, MSVC-OK):** `rusqlite 0.40.2` + `bundled` (vendored SQLite via `cc`), `clap 4.6.7` (`derive`), `tokio 1.53.1` (narrow: `rt-multi-thread macros sync time tracing` — NOT `full`), `thiserror 2.0.20`, `tracing 0.1.44` + `tracing-subscriber 0.3.23` (`fmt env-filter`).
- **Frontend:** `vue 3.5.42`, `vite 8.3.0` (engines `^20.19||>=22.12` → Node 24 OK), `@vitejs/plugin-vue 6.0.9`, `vue-tsc 3.3.11`, `pnpm 12.4.2`; TS: CTA template pins `~6.0.3`, latest `7.0.2` — hold at 6 until smoke-tested. `invoke` from `@tauri-apps/api/core`; file pickers via `plugin-dialog`. Tauri shell deferred to Phase 6 (core+CLI first).

## Decisions so far

- D1: Clean/hexagonal layers (Presentation → Application → Domain → Infrastructure, domain depends on nothing; ports in domain/application, adapters in infrastructure). Rationale: spec mandates it; keeps Tauri/CLI/SQLite swappable.
- D2: Start one modular Cargo workspace, split crates only when boundaries justify it (spec §33 warns against 7 crates on day one).
- D3: MSVC target, not GNU (Build Tools present; Tauri 2 + `rusqlite/bundled` + PDF renderers build reliably on MSVC).
- D4: No code before facts — image/PDF/Tauri crate choices wait for scout results (Step 3).

## Open questions — resolved (Step 3)

- Q1: `image` WebP encode quality — **insufficient** (VP8L lossless-only) → add `webp 0.3.1` (`libwebp-sys` native, MSVC OK) for lossy `encode(quality: f32)`.
- Q2: PDF→image renderer — **deferred**; stub returns structured `Unsupported`. Candidates: `hayro` (pure, experimental) vs `pdfium-render` (full-fidelity, ships DLL, binary license unverified).
- Q3: Tauri 2 MSVC requirement — **cited** (prerequisites + installer docs); versions `tauri 2.11.5` / CLI `2.11.4` / api `2.11.1` / dialog `2.7.3` / fs `2.5.2`.
- Q4: `rusqlite/bundled` on MSVC — **fine** (vendored SQLite via `cc`; MSVC present). Build time measured at first `cargo build` (Step 5).
