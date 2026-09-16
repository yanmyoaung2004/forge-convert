# ForgeConvert — Implementation Plan (Phase 0 → 10)

> Spec: `dev/product.md`. Architecture: clean layered + hexagonal
> (ports & adapters). Status: toolchain ready (rustc 1.98.1 MSVC), research
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
| hayro/pdfium (later) | PDF→image | pure vs fidelity — decide w/ license check | mupdf (AGPL✗), poppler (stale✗) | TBD w/ check |
