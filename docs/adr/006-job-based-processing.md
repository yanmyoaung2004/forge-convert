# ADR 006 — Job-Based Processing with Validated Transitions

- Status: accepted (2026-09-17)
- Spec: `product.md` §10–11; Master Spec §§6, 14–15

## Context

Scattered async conversions give no uniform progress, cancellation, naming,
history, or error reporting. Batch needs per-file states.

## Decision

Every operation is a `ConversionJob` (id, inputs, output config, options,
status, progress, timestamps, result). States
`Queued → Running → {Completed, Failed, Cancelled}` with a validated
transition table — invalid transitions return structured errors.
Pipeline per file: validate → detect → decode → normalize → transform →
encode → temp write → flush/sync → validate → atomic rename → Completed.
Default naming: same basename + new extension; collisions default to
Rename-auto (Replace/Skip/Fail are explicit).

## Alternatives

- Ad-hoc futures per button: rejected — no uniform observability.
- Persistent queue (DB-backed) from day one: rejected — in-memory first,
  persist history/results (Phase 9), not the live queue.

## Consequences

- UI/CLI poll or subscribe to the same job states; history reuses the record.
- Cancellation is a first-class transition, checked between stages.
