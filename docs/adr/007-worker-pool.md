# ADR 007 — Bounded Worker Pool for Batch

- Status: accepted (2026-09-17)
- Spec: `product.md` §§12–13; Master Spec §§12–13, 28

## Context

Image work is CPU- and memory-heavy. Spawning one task per file (1000
files → 1000 tasks, all decoded at once) exhausts memory and thrashes.

## Decision

Tokio `rt-multi-thread` + semaphore-bounded `spawn_blocking`
(CPU work off the async reactor): default concurrency = CPU count,
configurable; stream per file (decode → transform → encode → write →
release); per-file success/failure + aggregate progress + global
cancellation. Narrow tokio features (`rt-multi-thread macros sync time
tracing`) — never `full`.

## Alternatives

- Unbounded tasks: rejected — memory blowup (the spec calls this stupid).
- Rayon-only pool: rejected — no natural async progress/cancellation
  story for the UI; blocking pool composes better with job events.
- External queue (broker): rejected — local utility, in-process suffices.

## Consequences

- Benchmarks (Phase 10) find where concurrency stops helping (1/2/4/8
  workers on 100× 4K JPEG) instead of blindly maximizing threads.
