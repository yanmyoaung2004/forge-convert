# ForgeConvert — Implementation Plan (Phase 0 → 10)

> Spec: `product.md`. Architecture: clean layered + hexagonal
> (ports & adapters). Repo root is `dev/` — all paths in this plan are
> relative to it. Status: toolchain ready (rustc 1.98.1 MSVC), research
> done (`PROGRESS.md` → Research facts). This plan is the build order.
> Owner goal: **working product at the end** — every phase ends in a
> runnable, tested artefact, committed per task.

## Build order overview

```text
Phase 0  Architecture + ADRs + workspace skeleton        (this week)
Phase 1  Domain model (types, errors, validation) + unit tests
Phase 2  Image codecs: PNG/JPEG/WebP/BMP/TIFF (+ lossy WebP) + integration tests
Phase 3  Job engine: pipeline, atomic writes, naming, cancellation, progress
Phase 4  Batch: bounded worker pool (tokio), per-file + aggregate progress
Phase 5  PDF: image→PDF (printpdf) now; PDF→image stubbed Unsupported
Phase 6  Tauri 2 desktop UI (Vue 3 + TS + Vite) on tested core
Phase 7  CLI (clap) on same core — script-friendly, non-zero exits
Phase 8  Optimize: resize, quality, metadata policy, presets, web preset
Phase 9  History + settings in SQLite (rusqlite/bundled)
Phase 10 Hardening: malformed/large files, concurrency, benches, packaging
```

Each phase gates the next: `cargo fmt --check` + `cargo clippy -- -D warnings`
+ `cargo test --workspace` green before merging the phase commit.

## Hexagonal layout (single workspace first, split when justified)

```text
crates/
  forge-core/            # domain + ports (NO deps on tauri/sqlite/image crates
                         #   beyond types; pure logic, fully unit-tested)
    src/
      lib.rs             # re-exports
      domain/            # ConversionJob, ConversionRequest/Options, OutputTarget,
                         # ImageMetadata, ImageDimensions, PixelFormat, ColorSpace,
                         # FormatDescriptor/Capabilities, JobStatus/Progress,
                         # ConversionResult, Preset, BatchJob
      ports/             # traits: ImageDecoder, ImageEncoder, TransformStep,
                         # PdfWriter, PdfRenderer, JobRepository, FileSystem,
                         # HistoryStore, Clock, JobEventSink
      error.rs           # ForgeError enum (spec §16 + Master §16)
  forge-image/           # adapter: image 0.25 + webp 0.3.1 + kamadak-exif(later)
  forge-pdf/             # adapter: printpdf (write now); render stub Unsupported
  forge-engine/          # application: orchestrator, job manager, batch pool,
                         # naming, atomic-write, validation, presets
  forge-store/           # adapter: rusqlite/bundled history/settings (Phase 9)
apps/
  cli/                   # clap CLI (Phase 7)
  desktop/               # Tauri 2 + Vue 3 UI (Phase 6)
tests/
  integration/           # round-trips per spec §26
  fixtures/              # small.png, transparent.png, photo.jpg, logo.webp,
                         # sample.bmp, sample.tiff, multipage.pdf, malformed.pdf
  golden/                # property assertions (dims/format/alpha/pages/decodable)
docs/
  adr/                   # 001–012 (below)
  architecture/          # layers + pipeline diagrams
```

Dependency rule: `forge-core` depends on nothing internal; `forge-engine`
depends on `forge-core` only (ports, injected); adapters depend on core;
`apps/*` depend on engine + core. No adapter→adapter imports except via ports.
Tauri commands and CLI are thin: parse → call engine → render result.

## Phase 0 — Architecture + ADRs + skeleton (acceptance: workspace builds, ADRs merged)

- [ ] `cargo new` workspace (`Cargo.toml` members: forge-core, forge-engine,
      forge-image, forge-pdf, forge-store, apps/cli).
- [ ] ADRs `docs/adr/001–012`: tauri, rust-core, local-first, layered/hexagonal,
      canonical-image, jobs, worker-pool, sqlite-history, pdf-separation,
      cli-shared-core, no-ai-core, webp-lossy-via-libwebp-sys.
- [ ] `docs/architecture/layers.md` + pipeline diagram; `tests/fixtures/README.md`
      (fixture list + "keep small" rule).
- [ ] CI: `fmt + clippy -D warnings + test --workspace` on push.
- Done when: `cargo test --workspace` green on empty skeleton + ADRs committed.

## Phase 1 — Domain model (acceptance: unit tests for §26-unit list)

Types: `ImageFormat` (Png/Jpeg/WebP/Bmp/Tiff/Pdf), `FormatDescriptor`
(can_decode/can_encode/supports_alpha/lossless/lossy/metadata),
`FormatCapabilities::targets_for(format)`, `ConversionOptions`
(quality 1–100, resize, `MetadataPolicy::{Preserve,Remove}`,
`BackgroundPolicy` (default white — JPEG has no alpha), overwrite policy),
`OutputTarget`, `JobStatus` + validated transitions
(Queued→Running→Completed/Failed/Cancelled; invalid → structured error),
`ForgeError` (all §16 variants + `InvalidTransition`, `Unsupported`),
`output_path_for(input, format)` (same basename + new ext),
`parse_page_range`, preset config types.
Tests: format detection by magic bytes, option validation (incl. quality bounds,
RGBA→JPEG requires background), state-transition table, naming, capabilities.

## Phase 2 — Image codecs (acceptance: PNG/JPEG/WebP decode+encode round-trips)

- Decode via `image::load_from_memory_with_format` / guess; encode:
  PNG (`PngEncoder`), JPEG (`JpegEncoder::new_with_quality`),
  WebP-lossy (`webp::Encoder::from_rgba/…encode(quality f32)`),
  WebP-lossless (`image-webp`), BMP/TIFF via `image`.
- RGBA→JPEG composites over background first (never fake alpha).
- Re-encode strips metadata by default (documented; explicit policy in Phase 8).
- Tests: `tests/integration/image_roundtrips.rs` on generated fixtures
  (no binaries committed): PNG→JPEG/JPEG→PNG/PNG→WebP(q80)/WebP→PNG/
  TIFF,BMP→WebP; assert dimensions, format, alpha, decodability — never bytes.

## Phase 3 — Job engine (acceptance: single-file job end-to-end via engine API)

`ConversionOrchestrator::run(request, progress_cb, cancel_token)`:
validate → detect → decode → normalize → transform → encode → temp write →
flush/sync → validate → atomic rename → Completed. Cancellation checks between
stages (`Cancelled`). Collision policy: Rename-auto (default) / Replace / Skip
/ Fail → `OutputExists`. Structured `tracing` spans
(job_id, operation, duration, status, error category).

## Phase 4 — Batch (acceptance: 100-file batch, bounded, cancellable, exact report)

Tokio `rt-multi-thread` + semaphore-bounded `spawn_blocking`
(default = `num_cpus`, configurable): per-file success/failure, aggregate
progress callback, global cancellation, stream decode→…→release (never hold
whole batch). Report `73 ok / 27 failed` shape, never "something went wrong".

## Phase 5 — PDF (acceptance: images→PDF incl. multi-image; render stub tested)

Write now: `printpdf` (`PdfDocument::new`, `RawImage::decode_from_bytes`,
`XObjectTransform{scale,dpi}`, `PdfPage::new(Mm…)`); sizes A4
(`Mm(210)×Mm(297)`), Letter (`Mm(215.9)×Mm(279.4)`), portrait/landscape,
margins, fit/fill/center (caller-computed scale). Multi-image = one page each.
Render later: `PdfRenderer` port exists from day one, default impl returns
`ForgeError::Unsupported{capability:"pdf-to-image", hint:"…"}`; feature-gate
`hayro` vs `pdfium-render` decision (binary license check first).

## Phase 6 — Tauri desktop UI (acceptance: drag-drop → convert → progress → result)

Scaffold `apps/desktop` from `create-tauri-app` vue-ts (vue 3.5 / vite 8 /
TS ~6.0.3 / api 2.11.1 / plugin-dialog 2.7.3); commands only at job level:
`create_conversion_job/start_job/cancel_job/get_job_status/
get_format_capabilities/get_file_info` (spec §20). Backend authoritative;
frontend no business logic, no direct FS (dialog plugin for pickers).
Screens: Convert / Batch / PDF / Optimize / History / Settings — utility-simple.

## Phase 7 — CLI (acceptance: all §30 commands work, script-friendly exits)

`forgeconvert convert in.png --to webp [--quality 80]`, `batch ./dir …`,
`pdf img… --output doc.pdf`, `render doc.pdf --format png --dpi 200`
(surfaces Unsupported until renderer lands), `info f.png`, `optimize logo.png
--preset web`. Same engine calls as UI; non-zero exit per error variant.

## Phase 8 — Optimize (acceptance: web preset + before/after report)

`imageops::resize` (Lanczos3/Gaussian/Nearest choice), quality/compression,
`MetadataPolicy`, presets as config (`Web Optimized: webp q80 strip-meta`,
`High Quality JPEG`, …), web report (orig size → out size, % saved, dims,
format — measured, never claimed).

## Phase 9 — History + settings (acceptance: history survives restart)

`rusqlite/bundled`: jobs (id, op, in/out names, format, status, ts, duration,
options JSON), settings (defaults, overwrite, theme, workers, metadata policy,
recent folders). Files stay files — DB holds metadata only.

## Phase 10 — Hardening + release (acceptance: gates green, installers built)

Malformed/corrupt fixtures, missing-input, permission, OutputExists,
cancellation, large-file streaming, concurrency soak, `cargo bench`
(1/10/100 files, worker scaling 1/2/4/8), `tauri build` MSI+NSIS.
Definition of done per spec §35: impl + tests + errors + cancellation +
logging + docs + boundaries + fmt/lint/tests + no leak + no gratuitous dep.

## Dependency justification log (spec Rule 2 — every dep answers why)

| Crate (version) | Why | Why this lib | Alternatives rejected | License |
|---|---|---|---|---|
| image 0.25 | PNG/JPEG/BMP/TIFF + transforms, pure-Rust | de-facto std, maintained | codecs from scratch (never) | MIT OR Apache-2.0 |
| webp 0.3.1 | lossy WebP q-factor (`image-webp` lossless-only, verified) | only maintained safe wrapper w/ quality API | `mozjpeg`-style re-impl (no) | MIT OR Apache-2.0 (+libwebp C, MSVC OK) |
| printpdf 0.12.8 | image→PDF, pure-Rust | active, MIT, no native | reportlab-shellout (no shell) | MIT |
| rusqlite 0.40 (bundled) | history/settings | mature; bundled avoids sys-sqlite | sqlx (heavier), json file (no queries) | MIT (SQLite public-domain) |
| clap 4.6 (derive) | CLI | std for Rust CLIs | hand-rolled args (no) | MIT OR Apache-2.0 |
| tokio 1.53 (narrow) | bounded batch pool | work-stealing + spawn_blocking | rayon-only (no async progress/cancel) | MIT |
| thiserror 2.0 | structured errors | zero-boilerplate enums | string errors (banned) | MIT OR Apache-2.0 |
| tracing 0.1 + subscriber | structured logs w/ job spans | std for instrumented apps | log+env_logger (weaker spans) | MIT |
| tauri 2.11 | desktop shell | small bins, Rust backend | Electron (heavy) | Apache-2.0 OR MIT |
| kamadak-exif 0.6 (slice 3) | EXIF orientation read (auto-rotate) | pure-Rust, maintained, minimal | `rexif` (less maintained), `exif` crate (heavier API) | MIT |

## v0.2.0 — Installers work, batch matches convert, photos come out right, UI doesn't freeze

Theme: make v0.1.0's promises real. No new formats, no renderer decision (spike only, ship v0.3.0).
Order is load-bearing: slice 1 unblocks the release pipeline; slices 2–4 are independent features.
Each slice: code + tests + `fmt --check` + `clippy -D warnings` + `test --workspace` green, one commit.

### Slice 1 — Desktop build goes green (P0, unblocks everything)

- Commit `tauri-plugin-opener = { workspace = true }` in `apps/desktop/src-tauri/Cargo.toml`
  (staged but never committed; the Tauri CLI keeps rewriting the adjacent
  `tauri-build = { version = "2.6", features = [] }` line — revert that hunk, keep ours).
- Resolve `opener:default` vs `opener:allow-reveal-item-in-dir`: the committed capability
  lists `opener:default`, but local builds only resolved after switching to the explicit
  `opener:allow-reveal-item-in-dir`. Verify against the vendored
  `tauri-plugin-opener-2.5.5/permissions/` ACL manifest, pick the form that builds clean,
  commit it, and never touch the manifest again without re-verifying.
- Acceptance: `cargo check -p forgeconvert_desktop --offline` green from a clean worktree;
  `tauri dev` launches the window with no permission error; `Cargo.lock` committed with the
  opener tree (that's the +461-line `f20c59c` lesson: lockfile churn is expected, commit it).

### Slice 2 — Batch flag parity with convert

- Today `batch` takes only `--quality`; `convert` has the full surface
  (`--width/--height/--fit/--fill/--filter/--upscale/--png-level/--webp-lossless/--strip-metadata/--on-collision`).
- Reuse the existing `parse_resize` / `parse_compression` helpers: extract them from the
  `convert` arm into shared `fn`s (or a `CommonConvertArgs` struct), wire identical flags
  onto `Batch`, thread through `convert_one_sync`.
- Precedence and validation stay identical (explicit `quality` vs `Compression` rules).
- Tests: unit tests for shared parsers (bad `WIDTHxHEIGHT`, bad PNG level, lossless conflict);
  integration: batch over mixed fixtures with `--fit` + `--png-level` asserts per-file dims.
- Acceptance: `batch --help` shows the same resize/compression flags; a 3-file batch with
  `--fit 320x240` produces exactly 320x240-bounded outputs.

### Slice 3 — EXIF auto-rotate + metadata Preserve honesty

- Problem (verified gap): phone photos come out rotated (EXIF orientation ignored), and
  `Preserve` is a lie — re-encode already strips most metadata silently.
- Decoder: read EXIF orientation (pure-Rust crate, new dep justified in the table below)
  and apply the lossless transform (rotate/flip) during `canonicalize`, before any resize.
  Orientation 1 = no-op. Malformed EXIF → ignore + proceed (never fail the conversion).
- Encoder honesty: document that re-encode strips metadata by default; `Preserve` keeps
  what the target format round-trips (PNG text, JPEG EXIF via future work) and reports
  what was dropped where measurable. No silent claims.
- Tests: rotated-EXIF fixture decodes to upright dims; malformed EXIF doesn't fail;
  `info` reports orientation when present.
- Acceptance: a portrait phone JPEG converts to portrait output without flags.

### Slice 4 — Desktop: drag-drop, cancel, progress, savings

- Backend already supports it: orchestrator checks `cancel` between stages, batch reports
  per-file + aggregate progress, CLI prints before/after + % saved. The UI just doesn't
  expose any of it (picker-only, blocking convert, no sizes).
- Drag-drop: Tauri window file-drop events → append to `files` (same dedup + `inspectAll`
  path as picker). Guard: filter non-image extensions early, surface count of skipped.
- Cancel: `CancelFlag` per conversion, `cancel_job`-style command or shared atomic;
  Convert button becomes Cancel while `busy`; engine returns `Cancelled`, UI reports cleanly.
- Progress: stream per-file progress to the UI (Tauri events or polling `get_job_status`
  shape); at minimum a determinate bar (n/m files) instead of the static "Converting…" text.
- Savings: after convert, show per-file before → after bytes + % saved + dims (same data
  the CLI prints; needs output sizes — extend `ConvertDone` or follow-up `get_file_info`).
- Acceptance: drop 3 files → convert → cancel mid-run leaves clean state; completed run
  shows per-file sizes + savings; HMR screenshot proves layout.

### Slice 5 — Green CI + tag v0.2.0 with installers

- Full gates: `cargo fmt --check` + `clippy -D warnings` + `test --workspace` +
  `vue-tsc --noEmit` + `vite build` green locally, then push.
- Watch `ci` green on main, then `git tag v0.2.0 && git push origin v0.2.0`.
- `release` must go CLI matrix → `desktop-windows` (this time with bundles) → `publish`.
- Verify: 3 CLI assets + MSI + `-setup.exe` all `uploaded`; spot-download the Windows exe,
  run `--help` (exit 0). Update README download table to v0.2.0 links.
- Non-goal: PDF renderer ships v0.3.0 (timebox the hayro vs pdfium spike separately).
