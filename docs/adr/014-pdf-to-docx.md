# ADR 014 — PDF → Word via `lopdf` Text + `docx-rs`

- Status: accepted (2026-09-27)
- Spec: `docs/PLAN.md` v0.3.0 slice 1.

## Context

Users want editable Word output from PDFs. No renderer qualifies
(ADR 009/012), so pixel paths are out. What remains is the text layer:
extract per-page text, write paragraphs to OOXML.

## Decision

`forge-pdf::LopdfToDocx`: `load_mem` → per-page
`extract_text_with_limit(pages, 16 MiB)` (decompression-bomb-safe;
`MemoryLimitExceeded` → `ResourceLimitExceeded`) →
**`docx-rs 0.4`** builder (`Docx::new().add_paragraph(Paragraph::new()
.add_run(Run::new().add_text(line)))`, `page_break_before(true)` on each
page start except the first) → `build().pack(Cursor)`.

Scope is deliberately text-only: no images, no layout fidelity, one
paragraph per non-empty line. Scanned PDFs (no text layer) get a
`[No extractable text on page N]` marker paragraph — a valid .docx,
never an empty file, never an error. OCR is an explicit non-goal.

## Alternatives

- `pdf-extract 0.7`: rejected — pins `lopdf 0.34` (see ADR 013).
- Manual OOXML zip construction: rejected — reimplementing a document
  format; `docx-rs` is MIT, pure-Rust, minimal deps (`zip` + `serde`).
- `pandoc` shell-out: rejected — no shell-outs (local-first rule);
  external binary dependency breaks installers.
- `.doc` legacy output: rejected — obsolete format; `.docx` only.

## Consequences

- New `ImageFormat::Docx` variant (`docx`, Word MIME,
  `can_decode: false`, terminal `targets_for` = empty). Only
  `Pdf → Docx` exists; image encoder, batch filter, and desktop
  `convert_image` all reject `Docx` with pointer errors.
- `ImageFormat` stays the format enum (not a separate `DocumentFormat`)
  — one variant + match-arm sweep beats a parallel type hierarchy.
- License: `docx-rs` MIT, `lopdf` MIT — compatible with MIT OR Apache-2.0.
