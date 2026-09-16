# ADR 002 — Rust for the Conversion Core

- Status: accepted (2026-09-17)
- Spec: `product.md` §4; Master Spec §1

## Context

The core decodes/encodes binary image formats, renders/writes PDFs, streams
large files with bounded memory, and must expose one engine to a desktop UI
and a CLI. Toolchain verified: `rustc/cargo 1.98.1` (MSVC host).

## Decision

Implement the conversion core in **Rust** (edition 2021, MSRV 1.88 —
the highest floor among chosen deps). Never implement PNG/JPEG/WebP/PDF
codecs from scratch; use mature crates (justified in `docs/PLAN.md`).

## Alternatives

- Node/Python core: rejected — codec native-dep story weaker, binary
  distribution harder, memory control poorer for large batches.
- C++ core: rejected — memory-safety burden for untrusted-file parsing.

## Consequences

- Memory-safe handling of malformed inputs; `cargo` gates
  (fmt/clippy/test) enforce quality per phase.
- Native deps (`libwebp-sys`) need a C toolchain — satisfied by MSVC here.
