# ADR 011 — No AI/LLM in the Conversion Core

- Status: accepted (2026-09-17)
- Spec: `product.md` §29; Master Spec §§1, 32

## Context

LLM features (background removal, smart crop) are tempting, but the core
promise is deterministic, offline, dependency-light conversion.

## Decision

**No AI/LLM dependency in the core pipeline.** Same input + config →
same expected output properties (modulo codec behavior). Optional AI
features (if ever) live as separate, clearly-marked plugin subsystems —
never between decode and encode.

## Alternatives

- "Smart" auto-quality via model: rejected — unmeasurable claims, network
  weight, non-determinism; show measured before/after instead.
- Bundled inference runtime now: rejected — binary bloat, update burden.

## Consequences

- V2/V3 ideas (OCR, background removal) must arrive as isolated modules
  with their own ADRs, reusing the job/worker-pool machinery.
