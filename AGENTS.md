# Repository Guidelines

> Repo root is `dev/` (this file, `product.md`, `.git/` live here; parent `ForgeConvert/` holds only `dev/`). All paths below are relative to `dev/`.
> Status: spec read, toolchain + stack research done, plan written (`docs/PLAN.md`, `PROGRESS.md` → Research facts). Scaffolding follows `docs/PLAN.md` Phase 0 — spec + plan are authoritative; do not invent architecture.

## Project Overview

ForgeConvert: local-first, privacy-preserving desktop utility for image/PDF conversion, batch processing, optimization, and inspection. No uploads, no AI/LLM in the conversion pipeline, offline by default. Stack: Tauri 2 + Rust core + Vue 3 + TypeScript + Vite frontend + SQLite (history/settings/presets only; files stay on disk).

## Architecture & Data Flow

Clean layered + hexagonal (ports & adapters), depend inward only:

```text
apps/* (CLI, Tauri UI — thin: parse → call engine → render)
  → forge-engine (application: orchestrator, job manager, batch pool, naming, presets)
    → forge-core (domain + ports: NO internal deps, no Tauri/SQLite/codec deps)
  adapters → forge-core ports: forge-image, forge-pdf, forge-store
```

- **Canonical-image rule (hard):** no N×M converters. Every decoder (PNG/JPEG/WebP/TIFF/BMP) → canonical `ImageBuffer` → composable transform pipeline → encoder/PDF writer.
- **Single-image pipeline:** Validate → Detect → Decode → Normalize → Transform → Encode → temp write → flush/sync → validate → atomic rename → Completed.
- **Job lifecycle:** `Queued → Running → {Completed, Failed, Cancelled}`; invalid transitions → structured error.
- **Batch:** bounded worker pool (tokio semaphore + `spawn_blocking`); stream per file (`decode → transform → encode → write → release`); never hold the whole batch in memory.
- **PDF:** isolated subsystem. Image→PDF now (`printpdf`); PDF→image port exists from day one but default impl returns `Unsupported` (renderer deferred — see `docs/PLAN.md` Phase 5).
- **Errors:** `ForgeError` enum only (spec §16 variants + `InvalidTransition`, `Unsupported`); core never returns ad-hoc strings; UI/CLI map variants to messages.

## Key Directories

| Directory | Purpose / status |
|---|---|
| `product.md` | Product + master agent spec (2731 lines); authoritative spec |
| `docs/PLAN.md` | Phased build plan 0→10 + dep-justification table; authoritative build order |
| `PROGRESS.md` | Step-by-step dev record; update every task |
| `docs/adr/` | ADRs 001–012 (Phase 0); each Context/Decision/Alternatives/Consequences |
| `docs/architecture/` | Layer + pipeline docs (Phase 0) |
| `crates/forge-core/` | Domain types + ports + `ForgeError`; zero internal deps |
| `crates/forge-engine/` | Application layer; depends on `forge-core` only |
| `crates/forge-image/` | `image 0.25` + `webp 0.3.1` adapter |
| `crates/forge-pdf/` | `printpdf` writer + render stub |
| `crates/forge-store/` | `rusqlite/bundled` history/settings (Phase 9) |
| `apps/cli/` | `clap` CLI (Phase 7) |
| `apps/desktop/` | Tauri 2 + Vue 3 UI (Phase 6) |
| `tests/{integration,fixtures,golden}/` | Round-trips; generated fixtures preferred, committed binaries kept small |

## Development Commands

Shell caveat: this harness's Git-Bash cannot exec the cargo shims (symlink); run Rust via PowerShell:

```powershell
& "$env:USERPROFILE\.cargo\bin\cargo.exe" fmt --check
& "$env:USERPROFILE\.cargo\bin\cargo.exe" clippy --workspace -- -D warnings
& "$env:USERPROFILE\.cargo\bin\cargo.exe" test --workspace
& "$env:USERPROFILE\.cargo\bin\cargo.exe" test -p forge-core <filter> -- --exact
```

- Phase gate (every phase): `fmt --check` + `clippy -D warnings` + `test --workspace` green before commit.
- Git: repo root is `dev/` — run `git -C <…>/dev …`; one commit per task, `step-N:` prefix.
- Record: append a `PROGRESS.md` step-log row per completed task; `git log --oneline` is order truth.
- Frontend (Phase 6+): `pnpm` in `apps/desktop` (`dev`; `build = vue-tsc --noEmit && vite build`).

## Code Conventions & Common Patterns

- **Errors:** match `ForgeError` variants, never message text; `Result` everywhere; never hide failures.
- **Writes:** temp → flush/sync → validate → atomic rename; cleanup temps; default same-basename + new ext (`logo.png → logo.webp`); collisions: Rename-auto default, else `OutputExists`.
- **JPEG/alpha:** RGBA→JPEG composites over an explicit background (default white); never fake alpha.
- **Concurrency:** bounded pool only; per-file + aggregate progress; cancellation checks between stages → `Cancelled`.
- **Transforms:** composable steps (resize, quality, metadata policy, bg-composite, orientation); presets are config, not branches.
- **Contracts:** Tauri commands + CLI are thin; engine is authoritative; `forge-core` never imports Tauri/SQLite/codecs.
- **Logging:** `tracing` spans with `job_id, operation, in/out type, duration, status, error category`; never file bytes or sensitive metadata.
- **Style (spec §46):** no invented arch mid-implementation (stop + document); every new dep justified in the `docs/PLAN.md` table; no giant files; no speculative abstraction; boring/reliable over clever; no `TODO: implement` presented as done.
- **CLI surface:** `convert in.png --to webp`, `batch ./dir --to webp --quality 80`, `pdf imgs… --output doc.pdf`, `render doc.pdf --format png --dpi 200`, `info f.png`, `optimize logo.png --preset web`; non-zero exit per error variant.

## Important Files

- `product.md` — spec (stack §4, layers §5, pipeline §§7–16, CLI §§21/30, layout §33, tests §§26–27/35, agent rules §46, master spec §§1–36).
- `docs/PLAN.md` — build order + hexagonal layout + dep-justification table.
- `PROGRESS.md` — dev record + verified env/toolchain facts + stack research facts.
- `crates/forge-core/src/{domain,ports,error.rs}` — domain truth (Phase 1).
- `apps/desktop/src-tauri/{tauri.conf.json,capabilities/default.json}` (Phase 6).

## Runtime/Tooling Preferences

- Toolchain (verified 2026-09-17): `rustc/cargo 1.98.1`, host `x86_64-pc-windows-msvc`; MSVC 14.44 + WinSDK 10.0.26100.0 + WebView2 153 → Tauri 2 prerequisites satisfied.
- Rust deps (pin at scaffold; justify changes in PLAN table): `image 0.25` (PNG/JPEG/BMP/TIFF; bundled WebP encode is **lossless-only**), `webp 0.3.1` (lossy `encode(quality: f32)`; pulls `libwebp-sys` C — compiler present, OK), `printpdf 0.12.8` (image→PDF, pure-Rust), `rusqlite 0.40` + `bundled`, `clap 4.6` + `derive`, `tokio 1.53` (narrow `rt-multi-thread macros sync time tracing`, never `full`), `thiserror 2.0`, `tracing 0.1` + `tracing-subscriber 0.3` (`fmt env-filter`), `tauri 2.11` (never `3.0.0-alpha`). Rejected: `mupdf` (AGPL-3.0), `poppler` (stale, system-native).
- Frontend (Phase 6): `vue 3.5`, `vite 8`, `@tauri-apps/api 2.11`, `plugin-dialog 2.7`; TS pinned `~6.0.3` (CTA template; validate before 7.x); `invoke` from `@tauri-apps/api/core`; Node 24 + pnpm 12 OK.
- Constraints: local-first, offline default, no shell exec, path validation + safe temp dirs, no telemetry; SQLite holds metadata only (files stay files).

## Testing & QA

- `cargo test --workspace`: unit tests colocated (`#[cfg(test)] mod tests`); integration in `tests/`.
- Round-trips: PNG→JPEG, PNG→WebP(q80), JPEG→PNG, JPEG→WebP, WebP→PNG, TIFF/BMP→WebP, image(s)→PDF; PDF→image tests assert the `Unsupported` stub until a renderer lands.
- Golden: assert properties (dims, format, alpha, page count, decodability, approx size) — never byte equality (codec variance).
- Failure cases: corrupt/unsupported/missing input, inaccessible dirs, `OutputExists`, cancellation, malformed PDF; assert error variants, never message text.
- Naming: `test_<in>_to_<out>_<cond>` (e.g. `test_transparent_png_to_jpeg_flattens_alpha`).
- Fixtures: prefer generated; committed binaries stay small (`small.png`, `transparent.png`, `photo.jpg`, `logo.webp`, `sample.bmp`, `sample.tiff`, `multipage.pdf`, `malformed.pdf`).
- Gates: never delete tests to pass; never silently change public behavior; coverage thresholds TBD when the harness lands.
