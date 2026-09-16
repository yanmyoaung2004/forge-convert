# ADR 004 — Layered Modules with Hexagonal (Ports & Adapters) Influence

- Status: accepted (2026-09-17)
- Spec: `product.md` §§5–6; Master Spec §2. Map: `docs/architecture/layers.md`

## Context

Spec mandates separated responsibilities: UI → application services →
domain → ports → infrastructure. The classic failure is a Vue button
calling conversion code that touches SQLite and PDF code directly.

## Decision

One Cargo workspace, four conceptual layers, hexagonal influence:

- `forge-core`: domain types + ports + `ForgeError`. Depends on nothing
  internal, no Tauri, no SQLite, no codecs.
- `forge-engine`: application services; depends on `forge-core` only,
  ports injected (constructor injection, no service locator).
- Adapters (`forge-image`, `forge-pdf`, `forge-store`): implement core
  ports; depend on core; never on each other except via ports.
- `apps/*`: thin presentation adapters (parse → engine → render).

Start as workspace members (not micro-crates); split only when a boundary
earns it (spec §33 warns against 7 crates on day one — we keep 5 + CLI).

## Alternatives

- Flat single crate: rejected — boundaries unenforceable, UI logic leaks in.
- Microservices/IPC split: rejected — absurd for a local utility.
- Full DDD/CQRS/event-sourcing: rejected — disproportionate ceremony.

## Consequences

- `cargo` cannot enforce layering alone → code review checks the import
  rule (`forge-core` must stay dependency-free); CI runs clippy/test gates.
- Tauri/CLI/SQLite remain swappable behind ports.
