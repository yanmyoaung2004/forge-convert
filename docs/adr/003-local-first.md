# ADR 003 — Local-First, No Telemetry

- Status: accepted (2026-09-17)
- Spec: `product.md` §§3, 28; Master Spec §§17–18

## Context

Conversion tools that upload files create privacy, size-limit, and
offline problems. The product promise is: files never leave the machine.

## Decision

**Local-first, offline by default.** No upload, no account, no remote
conversion API, no telemetry/analytics. Update checks (if ever added) stay
separate from conversion. Structured logs never include file bytes or
sensitive metadata.

## Alternatives

- Server-assisted conversion: rejected — contradicts the product promise.
- Opt-out telemetry: rejected — default must be silent; revisit only with
  explicit opt-in design (new ADR).

## Consequences

- All codecs/engines run in-process; PDF renderers must be local too
  (hence rejecting cloud APIs outright in the renderer choice).
- Test strategy needs no network; CI stays hermetic.
