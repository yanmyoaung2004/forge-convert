# ADR 009 — PDF as an Isolated Subsystem

- Status: accepted (2026-09-17)
- Spec: `product.md` §§14–16; Master Spec §11

## Context

PDF is a document model (pages, layout, metadata), not an image. Forcing it
through the image pipeline conflates page layout with pixel transforms.

## Decision

`forge-pdf` is isolated from `forge-image`: own page model, layout
(A4/Letter, portrait/landscape, margins, fit/fill/center), writer, and
(future) renderer. Image→PDF: decode images → page layout → `printpdf`
writer. PDF→image: `PdfRenderer` port exists from day one; default impl
returns `ForgeError::Unsupported` until a renderer is qualified.

## Alternatives

- PDF-as-just-another-image-format: rejected — layout concepts leak into
  codecs; page ranges and DPI have no image equivalent.
- Shipping a renderer immediately: rejected — no option qualifies today
  (see ADR 012); a stubbed port with an honest error beats a bad renderer.

## Consequences

- Multi-image→PDF = one page per image; page size A4 `Mm(210)×Mm(297)`,
  Letter `Mm(215.9)×Mm(279.4)`; fit/fill computed by caller scale math.
- `lopdf` taken directly only if low-level ops (merge/split/metadata)
  outgrow `printpdf` (it is already printpdf's engine — no duplication).
