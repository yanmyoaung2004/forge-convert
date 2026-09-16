# ForgeConvert layers (clean + hexagonal influence)

Spec: `product.md` §§5–6, Master Spec §2. Full build order: `docs/PLAN.md`.

```text
apps/cli, apps/desktop/src-tauri   PRESENTATION (thin adapters)
  │  parse args / Tauri commands → call engine → render result
  ▼
crates/forge-engine                APPLICATION
  │  orchestrator, job manager, batch pool, naming, presets, validation
  │  depends on forge-core ONLY (ports, injected — never concrete adapters)
  ▼
crates/forge-core                  DOMAIN + PORTS (depends on nothing internal)
  │  domain/  ConversionJob, ConversionRequest/Options, OutputTarget,
  │           ImageMetadata/Dimensions, PixelFormat, ColorSpace,
  │           FormatDescriptor/Capabilities, JobStatus/Progress,
  │           ConversionResult, Preset, BatchJob
  │  ports/   ImageDecoder, ImageEncoder, TransformStep, PdfWriter,
  │           PdfRenderer, JobRepository, FileSystem, HistoryStore,
  │           Clock, JobEventSink
  │  error.rs ForgeError (structured; spec §16 + InvalidTransition + Unsupported)
  ▲
  │  implement ports; depend on core, never on each other (except via ports)
  │
crates/forge-image   image 0.25 + webp 0.3.1 (lossy) + exif (Phase 8)
crates/forge-pdf     printpdf writer now; PdfRenderer stub → Unsupported
crates/forge-store   rusqlite/bundled history + settings (Phase 9)
```

## Invariants

1. `forge-core` imports no workspace member, no Tauri, no SQLite, no codec crate.
2. No N×M converters: decode → canonical `ImageBuffer` → transform → encode.
3. All file finalization: temp → flush/sync → validate → atomic rename.
4. All failures are `ForgeError` variants; UI/CLI translate to messages.
5. New dependency → justify in `docs/PLAN.md` table first (spec Rule 2).
