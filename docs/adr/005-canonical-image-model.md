# ADR 005 — Canonical Image Model (No N×M Converters)

- Status: accepted (2026-09-17)
- Spec: `product.md` §§7–8; Master Spec §§3, 7

## Context

Five image formats naively means 20+ pairwise converters. That duplicates
transform logic (resize, metadata, alpha handling) in every path.

## Decision

Every decoder normalizes into one canonical `ImageBuffer`
(width, height, pixel format, color space, pixels, alpha, metadata);
one composable transform pipeline; then per-format encoders/PDF writer.
New formats add one decoder (+ encoder if writable) — never N new paths.
Implementation avoids needless large-buffer copies (move/borrow pixels).

## Alternatives

- Pairwise converters: rejected — N×M explosion, duplicated alpha/JPEG rules.
- Zero-copy-everywhere framework: rejected — over-engineering; bound memory
  at the batch level (stream per file) instead.

## Consequences

- Transform bugs are fixed once; golden tests assert post-transform
  properties on the canonical model.
- Initial pixel scope stays small (RGB8/RGBA8, then 16-bit as needed).
