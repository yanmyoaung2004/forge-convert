# Repository Guidelines

> **Status: pre-implementation, spec-only.** The repo contains one file (`dev/product.md`, ~2731 lines: product proposal + master agent spec). No `package.json`, `Cargo.toml`, source, tests, or configs exist yet. Everything below marked *(planned)* comes from that spec — do not assume it exists on disk. Update this file when scaffolding lands.

## Project Overview

ForgeConvert: local-first, privacy-preserving desktop utility for image/PDF conversion, batch processing, optimization, and inspection. No uploads, no AI/LLM in the conversion pipeline, offline by default. Proposed stack: Tauri 2 + Rust core + Vue 3 + TypeScript + Vite frontend + SQLite (history/settings/presets only; files stay on disk).

## Architecture & Data Flow

Planned layering *(spec §5)* — depend inward only:

```text
Vue UI → Tauri Commands (job-level) → Application → Domain → Infrastructure
         (create/start/cancel/status,   (Job Manager, Orchestrator,
          capabilities, file info)       Batch, Preset/History Managers)
                                         ↓
                                    Domain types: ConversionJob, ConversionOptions,
                                    ImageMetadata, OutputTarget, FormatCapabilities,
                                    JobStatus, structured Error enum
                                         ↓
                                    Infrastructure: codecs, PDF renderer/writer,
                                    filesystem, SQLite, bounded worker pool
```

- **Canonical-image rule (hard constraint):** N×M pairwise converters are prohibited. Every decoder (PNG/JPEG/WebP/TIFF/BMP) → canonical `ImageBuffer` (dimensions, pixel format, color space, pixels, alpha, metadata) → composable Transform Pipeline (resize, quality, metadata policy, bg-composite, orientation) → encoder/PDF writer.
- **Single-image pipeline:** Validate → Detect → Decode → Normalize → Transform → Encode → temp-file write → flush/sync → validate → atomic rename → Completed.
- **Job lifecycle:** `Queued → Running → {Completed, Failed, Cancelled}`; only validated transitions.
- **Batch:** bounded worker pool (configurable concurrency) over a job queue; stream per file (`decode → transform → encode → write → release`); never load the whole batch or spawn unbounded tasks.
- **PDF:** isolated subsystem (Reader/Renderer/Writer, page model, layout, metadata). Image→PDF inserts a Page Layout step; PDF→Image renders pages to images. Keep PDF code separate from image codecs.
- **Errors:** structured enum only — `UnsupportedFormat, InvalidFile, DecodeFailed, EncodeFailed, PdfReadFailed, PdfRenderFailed, PdfWriteFailed, PermissionDenied, DiskFull, OutputExists, Cancelled, InvalidConfiguration, ResourceLimitExceeded`. Core never returns ad-hoc strings; UI maps variants to messages.

## Key Directories

| Directory | Status |
|---|---|
| `dev/` | Exists; contains only `product.md` |
| `apps/desktop/src/` + `src-tauri/`, `apps/cli/` | *(planned, §33)* — not scaffolded |
| `crates/{domain,application,image-core,pdf-core,job-engine,storage,filesystem,infrastructure}/` | *(planned)* — explicitly: start as one modular crate, split only when justified |
| `tests/{integration,fixtures,golden}/` | *(planned)* — not present |
| `docs/{architecture,adr,development}/` | *(planned)* — ADRs `001–011`, each Context/Decision/Alternatives/Consequences |
| `src/`, `lib/`, `packages/` | Not used by the planned layout; do not create |

## Development Commands

```sh
# Nothing runnable yet — no package.json / Cargo.toml / Tauri / Vite configs exist.
# Expected once scaffolded (do NOT invent script names until configs land):
cargo fmt && cargo clippy -- -D warnings   # format + static checks (Rust)
cargo test --workspace                      # full Rust suite
cargo test <filter> -- --exact              # single Rust test
# Frontend runner TBD (expect Vitest once apps/desktop exists)
```

Agent workflow per spec: inspect repo → understand arch → find affected modules → read tests → smallest change → tests → `cargo fmt` + clippy/static → tests → diff review → docs/ADR if architecture changed. Phases run 0 (ADRs) → 10 (hardening/packaging); do not skip ahead.

## Code Conventions & Common Patterns

- **Errors:** match structured variants, never message text. Never hide errors (`Result` everywhere, no stringly-typed failures).
- **Writes:** temp → flush/sync → validate → atomic rename; cleanup temps; never overwrite unless explicitly configured. Default naming: same basename + new extension (`logo.png → logo.webp`, per spec).
- **Concurrency:** bounded worker pool only; per-file + aggregate progress; cancellation supported.
- **Pipeline shape:** `detect → decode → CanonicalImage → transform → encode → atomic finalize`. Presets (Web Optimized, High Quality JPEG, …) are config, not branches. JPEG RGBA requires explicit bg-composite default — never fake alpha.
- **Contracts:** Tauri commands stay at job level (`create_conversion_job`, `start_job`, `cancel_job`, `get_job_status`, `get_format_capabilities`, `get_file_info`); backend authoritative, frontend holds no business logic; domain depends on neither Tauri nor SQLite; CLI reuses the same core.
- **Logging:** structured, leveled (`ERROR/WARN/INFO/DEBUG/TRACE`) with `job_id, operation, input/output type, duration, status, error category`. Never log file contents, image/PDF bytes, or sensitive metadata.
- **Style rules (spec §46):** no invented architecture mid-implementation (stop + document instead); no unjustified dependencies; no duplicated business logic; no giant files; no speculative abstraction; boring/reliable over clever; no `TODO: implement`/stubs presented as done.
- **Planned CLI surface** *(spec §§21,30 — examples, not yet runnable):* `forgeconvert convert input.png --to webp`, `forgeconvert batch ./images --to webp --quality 80`, `forgeconvert pdf img1.png img2.jpg --output doc.pdf`, `forgeconvert render doc.pdf --format png --dpi 200`, `forgeconvert info image.png`, `forgeconvert optimize logo.png --preset web` (non-zero exit on error).

## Important Files

- `dev/product.md` — sole file; blocking reference for all scaffolding (stack §4, layers §5, pipeline §§7–16, CLI §§21,30, layout §33, tests §§26–27,35, conventions §46).
- **Missing (do not reference as if present):** `package.json`, `Cargo.toml`, `src-tauri/tauri.conf.json`, `vite.config.*`, `tsconfig.json`, `README.md`, `apps/desktop/src/`, `apps/cli/`, `crates/*/`, `tests/`, `docs/`, `Dockerfile`, CI workflows.

## Runtime/Tooling Preferences

- **Observed:** no runtime, package manager, or toolchain pinned — no `packageManager`/`engines`, lockfiles (`bun.lockb` / `package-lock.json` / `pnpm-lock.yaml`), `tsconfig*`, `eslint*`/`biome.json*`/`.prettier*`, `Dockerfile*`, `.github/workflows/*`.
- **Proposed:** Tauri 2 desktop, Rust backend (mature native image/PDF libs), Vue 3 + TS + Vite frontend, SQLite for metadata only.
- **Constraints:** layered modular monolith (hexagonal influence); no microservices; local-first; no shell exec; path validation + safe temp dirs; no telemetry.

## Testing & QA

**No tests, frameworks, or configs exist** (glob for `*.test.*`, `*.spec.*`, `tests/`, `__tests__/`, `vitest/jest/playwright/pytest` configs → no matches). Prescribed contract from spec, to replace with exact script names once scaffolding lands:

- **Rust core:** `cargo test` for unit + integration; `cargo bench`/Criterion-style benches for performance. Unit tests colocated (`#[cfg(test)] mod tests`) for format detection, option validation, job state transitions, output naming, presets, capability discovery.
- **Frontend:** runner TBD (expect Vitest); E2E runner, linter, coverage tool all TBD.
- **Integration paths** (`tests/integration/`): PNG→JPEG, PNG→WebP, JPEG→PNG, JPEG→WebP, WebP→PNG, image→PDF, multi-image→PDF, PDF→PNG/JPEG/WebP.
- **Fixtures** (`tests/fixtures/`, keep small, never commit huge binaries): `small.png`, `transparent.png` (alpha-flattening case), `photo.jpg`, `logo.webp`, `sample.bmp`, `sample.tiff`, `multipage.pdf`, `malformed.pdf` (must exercise error path, not crash).
- **Golden tests** (`tests/golden/`): assert *properties*, not bytes — dimensions, format, alpha behavior, page count, metadata policy, decodability, approx size. Byte-equality is an anti-pattern (codec variance).
- **Failure cases:** corrupt/unsupported/missing input, inaccessible dirs, existing output, cancellation, malformed PDF, large files where practical. Assert error *variants/codes*, never message text.
- **Naming** (prescribed pattern): `test_<input>_to_<output>_<condition>` — e.g. `test_png_to_webp_quality_80`, `test_transparent_png_to_jpeg_flattens_alpha`, `test_malformed_pdf_returns_structured_error`, `test_job_cancel_transitions_to_cancelled`.
- **QA gates:** format → static checks (clippy/tsc) → tests → diff review. Never delete tests to make a build pass; never silently change public behavior. Coverage thresholds: none declared — set explicitly when the harness lands.
