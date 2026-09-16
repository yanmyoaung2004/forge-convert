# Test fixtures

Prefer **generated** fixtures (created at test time in `std::env::temp_dir()`).
Commit binaries here only when generation cannot cover the case
(e.g. `malformed.pdf`, real-world EXIF samples).

Rules:

- Keep every committed file small (tens of KB, never MBs).
- Names are semantic and stable; tests depend on them:
  - `small.png` — tiny baseline RGB/RGBA image
  - `transparent.png` — must exercise alpha-flattening to JPEG
  - `photo.jpg` — photographic JPEG
  - `logo.webp` — lossy WebP sample
  - `sample.bmp`, `sample.tiff` — legacy inputs
  - `multipage.pdf` — 2+ pages for PDF→image and page ranges
  - `malformed.pdf` — must exercise the error path, never crash
- Golden tests assert **properties** (dimensions, format, alpha, page
  count, decodability), never byte-for-byte equality.
