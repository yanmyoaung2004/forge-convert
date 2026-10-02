# QR expansion — encode upgrades + decode + payload builders

> Fact-first plan (verified 2026-10-02, not guessed). Slices commit one by one.
> Gates every slice: `cargo fmt --check` + `clippy --workspace --all-targets -- -D warnings`
> + `cargo test --workspace --exclude forgeconvert_desktop` + `vue-tsc --noEmit` + `vite build`
> + `bun apps/desktop/src/tools/smoke.ts`.

## Why this scope (user picked: QR only, no general dev-toolbox)

- All local-first, all reuse `qrcode 0.14` + `image` already in tree. At most ONE new dep (decoder).
- Current QR is encode-only with hardcoded defaults — every slice below is a direct upgrade of it.

## Current state (read from tree, not assumed)

- Encode only: `QrCode::new` (default EC **M**), Luma PNG, size clamp 128–1024, 2048-byte cap
  (`apps/desktop/src-tauri/src/diagram.rs:92-104`, `apps/cli/src/main.rs:1054-1066`).
- Wiring: CLI enum `Qr` at `apps/cli/src/main.rs:249-262`, dispatch at `:1048-1084`;
  desktop `qr_png` at `diagram.rs:81-109`, registered in `src-tauri/src/lib.rs:31`;
  `api.qrPng` at `apps/desktop/src/api.ts:209-211`; UI `doQr` at `ToolsTab.vue:84-93`.
- Workspace: `Cargo.toml:21` = `qrcode 0.14, default-features=false, features=["image"]`
  → the `svg` render target is OFF (vendored `src/render/svg.rs` is `#![cfg(feature = "svg")]`).
- Vendored API (`qrcode-0.14.1`, read from registry src): `EcLevel::{L,M,Q,H}` (`types.rs:98-108`,
  L=7% M=15% Q=25% H=30% recovery), `Version::{Normal(1-40),Micro(1-4)}`,
  `with_error_correction_level` / `with_version`; render targets `image`
  (Luma/LumaA/Rgb/Rgba), `svg::Color`, `pic`, `string`, `unicode`.
- Decode: **nothing in tree** — `qrcode` is encode-only. Candidates from `cargo search`
  (all UNVERIFIED — license/MSVC/image-API TBD in spike): `rqrr 0.11.0`,
  `rxing 0.9.3` (zxing port, multi-format), `bardecoder 0.5.0`.
- Known wart: desktop writes a fixed temp path `forgeconvert-qr-{size}.png` (`diagram.rs:105`)
  — repeated generates overwrite each other.

## Slice A — encode upgrades (no new dep)

- `QrArgs` += `ec: Option<String>` (`L|M|Q|H` → `EcLevel`, default `M` = today's behavior),
  `format: Option<String>` (`png|svg`, default `png`). Bad values → `InvalidConfiguration` (CLI exit 2).
- Enable `svg` on the workspace `qrcode` dep (feature flag only, no new crate):
  `render::<svg::Color>().build() -> String`, atomic `.svg` write
  (reuse the `export_svg_file` non-SVG refusal pattern in reverse — refuse empty payload).
- PNG stays Luma; custom dark/light colors deferred unless trivial via `Rgb` in-slice.
- Quiet zone stays on (default); unique temp filename per generate (content hash or timestamp
  suffix — fixes the overwrite wart).
- CLI: `qr "text" [--ec H] [--format svg] [--output out.svg]`; desktop: EC dropdown +
  PNG/SVG toggle in the QR panel; `api.qrPng` gains opts.
- Tests: EC-H output still PNG-magic + expected dims; SVG output starts with `<svg`;
  invalid EC string → clean error. Rust tests for encode, bun untouched.
- **Commit:** `feat(qr): ec-level + svg output - encode options, CLI flags, desktop QR panel`

## Slice B — decode QR from images (one new dep, spiked first)

- Spike (throwaway, before committing): `cargo add rqrr`, verify license, MSVC build,
  decode a vendored QR PNG → text. Fallback order: `rqrr` → `rxing` → `bardecoder`.
  Record winner + license in `docs/PLAN.md` dep table before wiring.
- `qr_decode {path}` command: `StdFileSystem.read` (512 MiB cap) → grayscale via `image`
  → decoder → text. No-QR-in-image → structured `InvalidFile`/`DecodeFailed`
  (never panic, never empty string as success).
- CLI: `qr-decode img.png` prints decoded text to stdout (exit 4 when no QR found).
- Desktop: QR panel "Decode image" button via dialog picker → shows text or clean error.
- Tests: encode→decode round-trip; garbage bytes → clean error; valid photo without QR → clean error.
- **Commit:** `feat(qr): decode QR from images - <winner-crate>, qr-decode CLI, desktop decode`

## Slice C — payload builders + batch (pure TS + thin CLI)

- DOM-free builders in `apps/desktop/src/tools/texttools.ts` (bun-smoked):
  WiFi (`WIFI:T:WPA;S:..;P:..;;` with `;` escaping), URL (scheme check),
  `mailto:`, `sms:`, minimal vCard (`FN/TEL/EMAIL`). Each returns the exact payload string.
- Tools QR panel: payload-type tabs that fill the text box (preview + copy), encode path unchanged.
- CLI stays raw-text scriptable (`qr "WIFI:..."` already works); builders are TS-only by decision.
- Batch (include only if user confirms): `qr-batch list.txt --out-dir` → numbered PNGs
  through `resolve_explicit_output` + collision policy, per-line failures reported, never abort-all.
- Tests: builder vectors (WiFi escaping, vCard folding); bun smoke extended.
- **Commit:** `feat(qr): payload builders - wifi/url/vcard/mailto/sms (+ batch if scoped)`

## Slice D — version, gates, push, tag

- Bump workspace + tauri.conf + package.json `0.4.0 → 0.5.0`; README download table → v0.5.0 + QR bullets.
- Full gates green → commit → push → `git tag v0.5.0 && git push origin v0.5.0`
  → watch `release` → verify 5 assets.
- **Commit:** `chore(release): bump 0.4.0 to 0.5.0` then tag (tag is not a commit).

## Non-goals

- Styled/artistic QR (logo embed, custom eyes), explicit Micro-QR versions, non-QR barcodes
  (rxing fallback only if rqrr fails the spike), camera scanning (no camera plugin in tree),
  cloud/dynamic QR, general dev-toolbox items (declined by user).

## Risks (facts)

- Decoder dep: license + MSVC + `image`-buffer API all UNVERIFIED — spike decides rqrr vs rxing,
  never guess.
- `svg` feature expected pure-Rust string builder — verify no native deps on enable.
- `qrcode 0.14` is passively-maintained (registry `badges.maintenance`) — pin `0.14`, no upgrade churn.
