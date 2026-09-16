# ADR 008 — SQLite for History/Settings/Presets (Metadata Only)

- Status: accepted (2026-09-17)
- Spec: `product.md` §4 (persistence), §25; Master Spec §25

## Context

History, presets, settings, and recent folders need durable, queryable
storage. Files themselves must stay files.

## Decision

**SQLite via `rusqlite 0.40` with `bundled`** (vendored SQLite compiled by
`cc` — no system SQLite, no MSVC beyond what we have). Stores: job history
(id, op, in/out names, format, status, timestamp, duration, options JSON),
settings (defaults, overwrite policy, theme, worker count, metadata policy,
recent folders), presets. Never image/PDF bytes.

## Alternatives

- JSON/TOML files: rejected — ad-hoc queries (history search, stats) get
  painful; concurrent writes racy.
- sqlx / ORM: rejected — heavier than needed; `rusqlite` + small hand
  migrations suffice for a local settings DB.
- System SQLite: rejected — links via pkg-config/vcpkg, fragile on Windows.

## Consequences

- Phase 9 lands the store behind the `HistoryStore` port (engine tested
  with an in-memory fake until then).
- `bundled` lengthens first build (C compile) — acceptable, one-time cost.
