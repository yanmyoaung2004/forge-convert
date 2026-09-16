# ADR 010 — One Core, Two Frontends (Tauri UI + CLI Share the Engine)

- Status: accepted (2026-09-17)
- Spec: `product.md` §30; Master Spec §21

## Context

Desktop users want drag-drop + progress UI; developers/scripts want
`forgeconvert convert …` with pipes and exit codes. Two implementations of
conversion logic would drift.

## Decision

One engine (`forge-engine` + adapters), two thin frontends. CLI (`clap`
derive) exposes `convert`, `batch`, `pdf`, `render`, `info`, `optimize`
(spec §30 shapes); Tauri exposes job-level commands (ADR 001). Both map
the same `ForgeError` variants to messages/exit codes. No conversion logic
in either frontend.

## Alternatives

- UI-only, CLI later as fork: rejected — drift guaranteed.
- CLI shells out to the desktop binary: rejected — fragile IPC, no library.

## Consequences

- CLI is script-friendly: non-zero exits per error variant, machine-readable
  output modes where useful, progress on stderr.
- Build order puts CLI (Phase 7) right after the tested core so automation
  exists before UI polish.
