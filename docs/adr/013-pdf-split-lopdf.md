# ADR 013 — PDF Split via Direct `lopdf` (Not `printpdf`)

- Status: accepted (2026-09-27)
- Spec: `docs/PLAN.md` v0.3.0 slice 1; ADR 009 foresaw this
  ("`lopdf` taken directly only if low-level ops (merge/split/metadata)
  outgrow `printpdf`").

## Context

`printpdf 0.12` writes new PDFs (images → pages) but has no page-copy,
page-delete, or page-count API. Splitting needs exactly those: read the
page tree, keep a subset, write the survivors.

## Decision

`forge-pdf::LopdfSplitter` uses **`lopdf 0.44` directly** — the same
crate `printpdf` already depends on, so no new native code enters the
tree. Flow: `load_mem` → `get_pages` (1-based `BTreeMap<u32, ObjectId>`)
→ validate every requested page `<= total` (`InvalidConfiguration`,
never panic) → `delete_pages(complement)` (numbers refer to ORIGINAL
numbering — verified in `lopdf/src/processor.rs`) → `prune_objects`
→ `save_to(Vec)`.

## Alternatives

- Round-trip through `printpdf` (render pages → re-embed): rejected —
  no renderer qualified (ADR 009/012); re-encoding would rasterize
  vector content and bloat files.
- `pdf-extract 0.7` (bundles its own lopdf): rejected — pins
  `lopdf 0.34`, clashing with printpdf's `0.44` (two PDF object models
  in one tree).

## Consequences

- `lopdf` becomes a direct workspace dep (`default-features = false`,
  same flags printpdf uses) — lockfile gains no new native crates.
- `PageRange::parse` grammar (`1-3`, `1,3,5-7`) is shared with
  `render --pages`; out-of-range pages are config errors (exit 2),
  corrupt bytes are `PdfReadFailed` (exit 4).
