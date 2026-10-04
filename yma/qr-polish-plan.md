# QR polish + batch — `qr-batch`, history, reveal, quiet zone

> Fact-first plan (verified 2026-10-04, not guessed). Slices commit one by one.
> Gates every slice: `cargo fmt --check` + `clippy --workspace --all-targets -- -D warnings`
> + `cargo test --workspace --exclude forgeconvert_desktop` + `vue-tsc --noEmit` + `vite build`
> + `bun apps/desktop/src/tools/smoke.ts`.

## Why this scope (user picked: QR polish + batch)

- Everything below is a direct upgrade of current QR — no styled QR, no renderer, no toolbox creep.
- `qr-batch` was deferred from Slice C ("include only if user confirms") — now confirmed.
- History + reveal + quiet zone reuse existing patterns; no new deps anywhere.

## Current state (read from tree, not assumed)

- Encode: `ForgeImageEncoder::encode_qr(text, ec, format, size)` (`crates/forge-image/src/lib.rs:173-227`)
  — `QrCode::with_error_correction_level`, render `Luma` PNG (`min_dimensions`) or `svg::Color` string.
  Quiet zone always ON (renderer default `has_quiet_zone: true`); no toggle exists.
- CLI: `Qr` enum at `apps/cli/src/main.rs:249-268` (`text/output/size/ec/format/on_collision`),
  dispatch at `:1059-1082` (encode → `resolve_explicit_output` + `write_atomic` + print path).
  **Gap: no `record_history` in the `Qr` arm** — favicon (`:1049-1056`), pdf-split/merge/compress
  all record; `qr`/`qr-decode` do not. Exit codes via `ForgeError` (`InvalidConfiguration` → 2, etc.).
- Desktop: `qr_png {text,size?,ec?,format?}` at `diagram.rs:92-114` writes
  `temp/forgeconvert-qr-{hash16}-{size}.{ext}` (content-hash unique); `qr_decode {path}` at `:119-124`.
  Registered in `lib.rs:31-32`. `api.qrPng/qrDecode` (`api.ts:209-215`).
- Panel: `ToolsTab.vue:235-299` — payload tabs (raw/url/wifi/mailto/sms/vcard) → `qrText`,
  size/EC/format row, Generate → `qrMsg = "Wrote {path}"`, Decode image… → `qrDecoded` text.
  **Gap: no reveal-in-folder button** (pattern exists: `revealInFolder` in `api.ts:94-97`,
  used in `App.vue:304-308`). **Gap: no history refresh** after QR generate.
- Batch precedent: `convert_one_sync` + bounded pool live in CLI; batch over mixed files
  asserts per-file dims (`shared_options` parity pattern).

## Slice A — CLI `qr-batch` + history for `qr`/`qr-decode`

- New `QrBatch { list: PathBuf, out_dir: Option<PathBuf>, size, ec, format, on_collision }`:
  read list file (one payload per non-empty line, `#` comments skipped; empty list =
  `InvalidConfiguration` exit 2), per line `encode_qr` → `qr-{i:03}.{png|svg}` through
  `resolve_explicit_output` + `write_atomic` (same skip-echo as `Qr`); per-line failures to
  stderr, never abort-all; final `N ok / M failed / K skipped` summary on stderr.
- Add `record_history("qr", …, Png, …)` in `Qr` arm and `record_history("qr-decode", …)`
  in `QrDecode` arm (favicon pattern `:1049-1056`; `ImageFormat::Png` for qr files).
  qr-decode records input→input (text printed, no file written) — same as `info`-style ops.
- Tests: batch over 3-line fixture (incl. one over-2048 line) → 2 files + 1 failure counted;
  history smoke `qr` → `history` row. CLI exit codes unchanged.
- **Commit:** `feat(qr): qr-batch + history - list-file batch, record qr/qr-decode`

## Slice B — desktop reveal + quiet-zone toggle

- Panel: after Generate, show **Reveal** button calling existing `revealInFolder(path)`
  (`api.ts:94-97`); store last path in a `qrPath` ref alongside `qrMsg`.
  Refresh history list after successful generate (same `api.history(20)` pattern as other tabs)
  — only if history is cheap here; else skip with reason.
- Quiet zone: `encode_qr` += `quiet: Option<bool>` (default `true` = today's behavior);
  `render().quiet_zone(q)` threads through (vendored `Renderer::quiet_zone`, `render/mod.rs:106`).
  CLI `--no-quiet-zone` flag; `QrArgs` += `quiet: Option<bool>`; panel checkbox (default on).
  Scannability note: quiet zone off still decodes via `rqrr` in own tests (round-trip asserts it).
- Tests: `encode_qr(..., quiet=false)` PNG still decodes via `decode_qr` round-trip;
  size assertion relaxed for quiet-off (smaller canvas expected).
- **Commit:** `feat(qr): reveal + quiet-zone - reveal button, quiet toggle CLI/desktop`

## Slice C — version, gates, push, tag

- Bump workspace + tauri.conf + package.json `0.5.0 → 0.6.0`; README download table → v0.6.0
  + `qr-batch` example + history/reveal/quiet-zone bullets.
- Full gates green → commit → push → `git tag v0.6.0 && git push origin v0.6.0`
  → watch `release` → verify 5 assets.
- **Commit:** `chore(release): bump 0.5.0 to 0.6.0` then tag (tag is not a commit).

## Non-goals

- Styled/artistic QR (logo embed, custom colors), explicit Micro-QR versions, non-QR barcodes,
  camera scanning, cloud/dynamic QR, `qr-batch` from stdin/URLs, general dev-toolbox items.

## Risks (facts)

- `qrcode 0.14` passively-maintained — pin, no upgrade churn (same as qr-plan).
- Batch list file with CRLF/UTF-8-BOM: trim + strip BOM or Windows-authored lists break line 1.
- `record_history` is best-effort (never fails conversion) — history assertions must tolerate
  in-memory fallback.
