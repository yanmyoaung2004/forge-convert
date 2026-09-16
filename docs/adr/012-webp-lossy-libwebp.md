# ADR 012 — Lossy WebP via `webp` (libwebp) — `image-webp` Is Lossless-Only

- Status: accepted (2026-09-17)
- Evidence: `image-webp v0.2.4` `src/encoder.rs` — `WebPEncoder::new`
  documented "Only supports VP8L lossless encoding" (verified from GitHub
  raw 2026-09-17); `image 0.25.10` routes `webp` feature to that encoder.
  `webp 0.3.1` exposes `Encoder::from_rgb/from_rgba/from_image` +
  `encode(quality: f32)`, `encode_lossless`, `encode_simple/advanced`
  (verified from repo source).

## Context

The "Web Optimized" preset (WebP q80) is the flagship workflow. The pure-Rust
`image` stack can *decode* WebP but only *lossless-encode* it — quality 80
lossy output is impossible without a VP8 encoder.

## Decision

Decode WebP via `image`; encode lossy WebP via **`webp 0.3.1`**
(`libwebp-sys 0.9.3` native C). Lossless path may keep `image-webp`.
`forge-image` hides both behind the `ImageEncoder` port so callers never
see which encoder served the request.

## Alternatives

- Lossless-only WebP: rejected — file sizes miss the optimization goal.
- `mozjpeg`-style custom VP8: rejected — reimplementing a video codec,
  absurd.
- Pure-Rust lossy WebP encoder crate: none mature found — revisit if one
  appears (new ADR).

## Consequences

- One native C dep in the tree; needs a C compiler at build time —
  satisfied (MSVC 14.44). Prebuilt/binary distribution unaffected.
- License: `webp` crate MIT OR Apache-2.0; underlying libwebp is
  BSD-3-Clause (permissive) — compatible with our MIT OR Apache-2.0.
