Yes. And I would make one important correction to the way you're framing it:

**Don't build "a website that converts files." Build a local-first document and image processing utility with a reusable conversion engine.**

That distinction matters because the second architecture gives you room to grow without turning the project into a pile of conversion-specific code.

Since you want an AI coding agent to implement it with minimal human intervention, the specification needs to be unusually explicit: architecture, boundaries, contracts, invariants, error handling, testing, UX behavior, and implementation phases.

Below is the proposal I would actually give to the coding agent.

---

# Project Proposal: ForgeConvert

**Working name:** `ForgeConvert`

> A privacy-first, local-first desktop utility for converting, transforming, inspecting, optimizing, and batch-processing images and PDF documents without uploading files to external services.

The name is not important. The architecture is.

---

# 1. Executive Summary

ForgeConvert is a cross-platform desktop application that provides local file conversion and document-processing capabilities.

The primary use cases are:

- PNG → JPEG
- JPEG → PNG
- PNG → WebP
- JPEG → WebP
- WebP → PNG/JPEG
- TIFF/BMP → modern formats
- Image → PDF
- Multiple images → one PDF
- PDF → PNG/JPEG/WebP
- PDF pages → individual images
- Image resizing
- Image compression
- Image optimization
- Image metadata inspection/removal
- Batch conversion
- Drag-and-drop processing
- File information inspection

The application must operate **locally by default**.

No uploaded files should leave the user's computer during normal operation.

---

# 2. Why This Project Exists

The problem is simple:

Many common file-conversion tasks are artificially inconvenient.

A user may need to:

```text
logo.png
   ↓
logo.webp
```

or:

```text
10 scanned images
   ↓
one PDF
```

or:

```text
document.pdf
   ↓
page-001.png
page-002.png
page-003.png
```

Existing online tools frequently introduce:

- upload/download overhead
- file-size restrictions
- daily limits
- advertisements
- account requirements
- privacy concerns
- internet dependency

ForgeConvert eliminates the dependency on online conversion services for common operations.

---

# 3. Product Philosophy

The product should follow these principles:

### 1. Local-first

Files stay on the user's machine.

### 2. Fast

Use native libraries and parallel processing where appropriate.

### 3. Simple

A user should be able to drag files in, select an output format, and convert.

### 4. Powerful

Advanced options should exist without overwhelming the basic workflow.

### 5. Deterministic

The same input + configuration should produce the same expected result, subject to codec/library behavior.

### 6. Scriptable

A CLI should eventually expose the same conversion engine.

### 7. Extensible

Adding a new format should not require rewriting the entire application.

### 8. Observable

Long-running/batch operations should provide meaningful progress and errors.

---

# 4. Recommended Technology Stack

I'm deliberately choosing a stack rather than giving you ten alternatives.

## Desktop

**Tauri 2**

Why:

- small binaries compared with Electron
- native desktop integration
- Rust backend
- cross-platform
- you already have experience with it

## Backend/Core

**Rust**

This is the correct choice here.

Reasons:

- native performance
- memory safety
- excellent filesystem APIs
- concurrency
- good CLI ecosystem
- good fit for image/PDF native libraries
- easy to expose a reusable core
- suitable for future performance optimization

## Frontend

**Vue 3 + TypeScript**

You already know this combination from your Tauri work.

Use:

- Vue 3
- TypeScript
- Vite
- a lightweight component system
- CSS rather than an unnecessarily huge UI framework

Don't build a frontend architecture larger than the application.

## Local persistence

**SQLite**

But here's an important point:

### SQLite should NOT store the actual files.

Files remain files.

SQLite stores:

```text
conversion history
settings
job metadata
presets
recent folders
application preferences
```

---

# 5. High-Level Architecture

Use a **layered modular architecture with a hexagonal/ports-and-adapters influence**.

Do NOT use microservices.

That would be ridiculous for a local desktop application.

Architecture:

```text
┌──────────────────────────────────────────────┐
│                 Presentation                 │
│                                              │
│            Vue 3 + TypeScript                │
│                                              │
│  Dashboard / Converter / Batch / Settings    │
└──────────────────────┬───────────────────────┘
                       │
                 Tauri Commands
                       │
┌──────────────────────▼───────────────────────┐
│              Application Layer               │
│                                              │
│ Job Manager                                  │
│ Conversion Orchestrator                      │
│ Batch Processor                              │
│ Preset Manager                               │
│ History Manager                              │
└──────────────────────┬───────────────────────┘
                       │
┌──────────────────────▼───────────────────────┐
│                  Domain                      │
│                                              │
│ ConversionJob                                │
│ ConversionOptions                            │
│ ImageMetadata                                │
│ OutputTarget                                 │
│ FormatCapabilities                           │
│ JobStatus                                    │
│ Errors                                       │
└──────────────────────┬───────────────────────┘
                       │
┌──────────────────────▼───────────────────────┐
│               Infrastructure                 │
│                                              │
│ Image codecs                                 │
│ PDF renderer                                 │
│ PDF writer                                   │
│ Filesystem                                   │
│ SQLite                                      │
│ Worker pool                                  │
│ OS integration                               │
└──────────────────────────────────────────────┘
```

---

# 6. Why This Architecture?

Because there are several things you absolutely don't want.

You don't want:

```text
Vue button
 ↓
Rust function
 ↓
PNG conversion code
 ↓
SQLite
 ↓
PDF code
```

all mixed together.

That becomes unmaintainable quickly.

Instead:

```text
UI
 ↓
Application service
 ↓
Domain operation
 ↓
Port
 ↓
Infrastructure implementation
```

This gives you clean boundaries.

---

# 7. Core Architecture Decision: Conversion Pipeline

This is the most important design decision.

Do NOT implement:

```text
PNG → JPEG
PNG → WebP
PNG → PDF
JPEG → PNG
JPEG → WebP
JPEG → PDF
WebP → PNG
...
```

as separate workflows.

That creates an N × M problem.

Instead:

```text
              ┌── PNG decoder
              ├── JPEG decoder
Input ────────┼── WebP decoder
              ├── TIFF decoder
              └── BMP decoder
                       │
                       ▼
               Common Image Model
                       │
                Transform Pipeline
                       │
                       ▼
              ┌── PNG encoder
              ├── JPEG encoder
              ├── WebP encoder
              ├── TIFF encoder
              └── PDF writer
```

This is the architecture that matters.

---

# 8. Canonical Internal Representation

Once an image is decoded, normalize it into a common representation.

Conceptually:

```text
ImageBuffer
├── width
├── height
├── pixel_format
├── color_space
├── pixels
├── alpha
└── metadata
```

For example:

```text
PixelFormat:
    RGB8
    RGBA8
    RGB16
    RGBA16
```

You don't need to support every pixel format initially.

Start with sensible formats.

The important concept is:

> **Every supported input format gets converted into the internal representation before transformation/output.**

---

# 9. Format Abstraction

Create a format capability system.

Conceptually:

```text
FormatDescriptor

name
extensions
mime_types
can_decode
can_encode
supports_alpha
supports_animation
supports_lossless
supports_lossy
supports_metadata
```

Then the UI can ask the backend:

```text
"What formats can I convert this file to?"
```

rather than hardcoding assumptions.

---

# 10. Architecture of a Conversion Job

Every operation should become a job.

Example:

```text
ConversionJob
├── id
├── input_files
├── output_format
├── output_directory
├── options
├── status
├── progress
├── created_at
└── result
```

Status:

```text
Queued
Running
Completed
Failed
Cancelled
```

This is much better than having random asynchronous operations scattered throughout the codebase.

---

# 11. Job Pipeline

For a single image:

```text
Input
 ↓
Validate
 ↓
Detect format
 ↓
Decode
 ↓
Normalize
 ↓
Transform
 ↓
Encode
 ↓
Write temporary output
 ↓
Atomic rename
 ↓
Completed
```

That last part is important.

Don't directly overwrite the final file while writing.

Use:

```text
output.tmp
   ↓
successful write
   ↓
atomic rename
   ↓
output.webp
```

This prevents partially-written files from being presented as valid outputs.

---

# 12. Batch Processing Architecture

Batch operations should use a bounded worker pool.

Not:

```text
1000 files
 ↓
spawn 1000 tasks
```

That's stupid.

Instead:

```text
                 Job Queue
                     │
        ┌────────────┼────────────┐
        ↓            ↓            ↓
     Worker 1     Worker 2     Worker 3
        │            │            │
        ▼            ▼            ▼
      Decode       Decode       Decode
        │            │            │
        ▼            ▼            ▼
      Encode       Encode       Encode
        │            │            │
        ▼            ▼            ▼
      Output       Output       Output
```

Worker count should be configurable or automatically determined.

---

# 13. Memory Strategy

This matters enormously for image processing.

Never load a massive batch into memory simultaneously.

Bad:

```text
500 images
 ↓
load all
 ↓
convert
```

Better:

```text
File
 ↓
Decode
 ↓
Transform
 ↓
Encode
 ↓
Write
 ↓
Release
 ↓
Next
```

For large images, memory usage should be bounded as much as practical.

---

# 14. PDF Architecture

Treat PDF as a separate subsystem.

Don't force PDF to behave exactly like an image.

Architecture:

```text
PDF Module
├── PDF Reader
├── PDF Renderer
├── PDF Writer
├── Page Model
├── Page Layout
└── PDF Metadata
```

---

# 15. Image → PDF

Pipeline:

```text
Images
 ↓
Decode
 ↓
Normalize
 ↓
Page Layout
 ↓
PDF Writer
 ↓
PDF
```

Support:

- A4
- Letter
- custom size
- portrait
- landscape
- fit to page
- fill page
- centered
- margins
- one image per page
- multiple images per page eventually

---

# 16. PDF → Image

Pipeline:

```text
PDF
 ↓
PDF Parser
 ↓
Page
 ↓
Renderer
 ↓
Pixel Buffer
 ↓
Image Encoder
 ↓
Output
```

Support:

```text
PNG
JPEG
WebP
```

with configurable:

```text
DPI
quality
page range
output naming
```

Example:

```text
document.pdf

→ page-001.png
→ page-002.png
→ page-003.png
```

---

# 17. Image Optimization

This is one feature I strongly recommend adding.

Your real-world website example makes this useful.

Imagine:

```text
logo.png
5.4 MB
```

You want:

```text
logo.webp
120 KB
```

So add an optimization workflow:

```text
Image
 ↓
Analyze
 ↓
Optimize
 ↓
Compress
 ↓
Output
```

Options:

- quality
- max width
- max height
- output format
- lossless/lossy
- metadata removal

---

# 18. "Optimize for Web" Preset

Add a one-click preset:

> **Web Optimized**

Example:

```text
Input:
logo.png

Output:
logo.webp

Rules:
- preserve dimensions
- WebP
- quality 80
- remove unnecessary metadata
```

Later add:

> **Web Optimized — AVIF**

if the underlying libraries support it reliably.

This directly addresses your website-development workflow.

---

# 19. Image Inspection

Add an information panel.

For example:

```text
logo.png

Format       PNG
Dimensions   2048 × 2048
Color        RGBA
Alpha        Yes
Bit Depth    8
File Size    3.2 MB
Metadata     Present
```

This is useful enough to justify keeping it in the application.

---

# 20. Metadata Management

Support:

### View metadata

```text
EXIF
ICC
XMP
PNG metadata
```

### Remove metadata

Useful for:

- privacy
- website optimization
- reducing unnecessary file size

But don't blindly destroy metadata during every conversion.

Make it explicit:

```text
Preserve metadata
Remove metadata
```

---

# 21. Drag and Drop

This should be a first-class workflow.

User:

```text
Drag image
      ↓
Application
      ↓
Detected:
PNG
2048×2048
RGBA
      ↓
Suggested:
WebP
      ↓
Convert
```

For multiple files:

```text
Drag 50 images
       ↓
Batch conversion
       ↓
Choose output
       ↓
Convert
```

---

# 22. Presets

Allow saved presets.

Examples:

```text
WebP Website
JPEG Email
PNG Transparent
PDF A4
PDF High Quality
PDF Small Size
```

Internally:

```text
Preset
├── name
├── output_format
├── quality
├── resize
├── metadata_policy
├── pdf_settings
└── naming_policy
```

---

# 23. Smart Output Naming

This is a small feature that will save you irritation.

Example:

```text
logo.png
```

→

```text
logo.webp
```

Batch:

```text
IMG_001.png
IMG_002.png
IMG_003.png
```

→

```text
IMG_001.webp
IMG_002.webp
IMG_003.webp
```

Allow:

```text
{filename}
{extension}
{index}
{date}
```

Eventually:

```text
{filename}-optimized.{extension}
```

---

# 24. Collision Handling

Never silently overwrite files.

Options:

```text
Replace
Skip
Rename automatically
Ask
```

Default:

**Rename automatically or ask.**

For an automated CLI, use explicit flags.

---

# 25. Undo / Recovery

You don't need full Photoshop-level undo.

But conversion history should record:

```text
Input
Output
Options
Timestamp
Status
```

If a batch operation fails halfway:

```text
100 files
 ↓
73 successful
 ↓
27 failed
```

the application should report exactly that.

Not:

> "Something went wrong."

That's useless.

---

# 26. Error Architecture

Create structured errors.

For example:

```text
UnsupportedFormat
InvalidFile
DecodeFailed
EncodeFailed
PermissionDenied
DiskFull
OutputExists
PdfRenderFailed
Cancelled
MemoryLimitExceeded
```

The UI translates these into human-readable messages.

The Rust core should not return random strings everywhere.

---

# 27. Security

Even though this is a local application, files are untrusted input.

Treat every input file as potentially malformed.

You need:

- format validation
- file-size limits where appropriate
- safe temporary directories
- path validation
- no arbitrary shell execution
- no automatic execution of files
- safe output paths
- protection against path traversal
- cleanup of temporary files
- graceful handling of malformed PDFs/images

This is particularly important because image/PDF parsers process complex binary formats.

---

# 28. Privacy Architecture

Default behavior:

```text
Internet
   X
   │
ForgeConvert
   │
Local filesystem
```

No:

- telemetry
- analytics
- cloud upload
- remote conversion API
- account requirement

If you later add update checking, make it separate from conversion.

---

# 29. No AI in the Core

This is another decision I'm making firmly.

**Do not put an LLM into this application.**

You don't need one.

A conversion engine doesn't become better because GPT is sitting between PNG and JPEG.

You could eventually add optional AI features such as:

> "Remove background"

or

> "Automatically crop the subject"

but those belong to a separate optional subsystem.

The core conversion pipeline should remain deterministic and local.

---

# 30. CLI Architecture

Eventually:

```text
forgeconvert convert input.png --to webp
```

Batch:

```text
forgeconvert batch ./images --to webp --quality 80
```

PDF:

```text
forgeconvert pdf images/*.png --output document.pdf
```

PDF rendering:

```text
forgeconvert render document.pdf --format png --dpi 200
```

Information:

```text
forgeconvert info image.png
```

Optimization:

```text
forgeconvert optimize logo.png --preset web
```

The CLI should call the **same Rust application/domain layer**.

Do not duplicate conversion logic.

---

# 31. GUI Architecture

The GUI should be a consumer of backend APIs.

```text
Vue
 │
 ▼
Tauri Commands
 │
 ▼
Rust Application Layer
 │
 ▼
Domain
 │
 ▼
Infrastructure
```

The frontend should NOT:

- manipulate files directly unnecessarily
- implement image conversion
- duplicate validation logic
- contain business rules
- decide format capabilities independently

The backend is authoritative.

---

# 32. Suggested UI

Keep it simple.

### Main screen

```text
┌─────────────────────────────────────────────┐
│ ForgeConvert                                │
├─────────────────────────────────────────────┤
│                                             │
│       Drop files here                      │
│                                             │
│       or [Choose Files]                    │
│                                             │
├─────────────────────────────────────────────┤
│ Output: [ WebP ▼ ]                          │
│                                             │
│ Quality: [──────●──] 80                     │
│                                             │
│ [ Advanced Options ]                        │
│                                             │
│              [ Convert ]                    │
└─────────────────────────────────────────────┘
```

Then separate pages:

```text
Convert
Batch
PDF
Optimize
History
Settings
```

Don't build a giant dashboard.

This is a utility. **It should get out of the user's way.**

---

# 33. Project Structure

I'd recommend something approximately like:

```text
forgeconvert/
│
├── apps/
│   ├── desktop/
│   │   ├── src/
│   │   └── src-tauri/
│   │
│   └── cli/
│
├── crates/
│   ├── domain/
│   ├── application/
│   ├── image-core/
│   ├── pdf-core/
│   ├── job-engine/
│   ├── storage/
│   ├── filesystem/
│   └── infrastructure/
│
├── tests/
│   ├── integration/
│   ├── fixtures/
│   └── golden/
│
├── docs/
│   ├── architecture/
│   ├── adr/
│   └── development/
│
└── README.md
```

You don't have to literally create a Cargo workspace with seven crates on day one.

In fact, **don't**.

Start with a modular Rust workspace/package structure and split into crates when boundaries justify it.

This is one place where overengineering would hurt you.

---

# 34. Architecture Decision Records

Your coding agent should create ADRs.

For example:

```text
docs/adr/

001-tauri-desktop.md
002-rust-core.md
003-local-first.md
004-layered-architecture.md
005-canonical-image-model.md
006-job-based-processing.md
007-worker-pool.md
008-sqlite-history.md
009-pdf-subsystem.md
010-cli-shared-core.md
011-no-ai-core.md
```

Each ADR should contain:

```text
Context
Decision
Alternatives
Consequences
```

This will actually help you learn architecture while the AI builds the application.

---

# 35. Testing Strategy

This project should have serious automated testing.

### Unit tests

Test:

- format detection
- option validation
- naming
- path generation
- presets
- job state transitions

### Integration tests

Test:

```text
PNG → JPEG
PNG → WebP
JPEG → PNG
PDF → PNG
Images → PDF
```

### Golden tests

Use known fixtures.

For example:

```text
fixtures/
├── small.png
├── transparent.png
├── photo.jpg
├── logo.webp
├── multipage.pdf
└── malformed.pdf
```

Compare output properties.

Don't rely exclusively on byte-for-byte equality because codecs may have implementation-dependent output.

Check:

- dimensions
- format
- alpha behavior
- page count
- metadata policy
- decodability
- approximate expected size where appropriate

---

# 36. Performance Testing

Benchmark:

```text
1 image
10 images
100 images
1000 images
```

Measure:

- total time
- throughput
- peak memory
- CPU utilization
- output size

For example:

```text
Input:
100 × 4K JPEG

Measure:
Sequential
2 workers
4 workers
8 workers
```

Then determine where concurrency stops helping.

**Do not blindly maximize thread count.**

---

# 37. Observability

Even a desktop application benefits from structured logging.

Use levels:

```text
ERROR
WARN
INFO
DEBUG
TRACE
```

Example:

```text
job=8f2...
input=logo.png
operation=image_to_webp
duration=142ms
status=success
```

Never log sensitive file contents.

---

# 38. Configuration

Use a local configuration file/database.

Settings:

```text
Default output format
Default quality
Default output directory
Overwrite behavior
Theme
Worker count
Metadata policy
Recent folders
```

---

# 39. Future Features

Don't implement these initially, but design so they can eventually exist.

### Image

- AVIF
- GIF
- animated WebP
- SVG rasterization
- ICO
- HEIC/HEIF where platform/licensing support permits
- image resizing
- cropping
- rotation
- watermarking
- contact sheets

### PDF

- PDF merging
- PDF splitting
- PDF compression
- page reordering
- page extraction
- PDF metadata editing
- image-to-PDF OCR later

### Utility

- duplicate detection
- file hashing
- checksum generation
- image comparison
- before/after size comparison

---

# 40. One Particularly Useful Feature: "Convert & Optimize"

I would prioritize this because of your web-development use case.

Imagine dropping:

```text
logo.png
hero.jpg
icon.png
banner.tiff
```

The app analyzes them and shows:

```text
File        Current       Recommended
logo.png    3.2 MB        WebP 180 KB
hero.jpg    2.1 MB        WebP 420 KB
icon.png    400 KB        WebP 35 KB
banner.tiff 8.7 MB        WebP 600 KB
```

Then:

**Optimize All**

This is far more useful than simply recreating "Convert PNG to JPEG."

---

# 41. Another Useful Feature: Before/After

Show:

```text
Original
2.8 MB
2048 × 2048

        ↓

WebP
184 KB
2048 × 2048

Savings: 93.4%
```

This gives the user immediate feedback.

---

# 42. Product Scope

## MVP

The first version should contain only:

### Image

- PNG
- JPEG
- WebP
- BMP
- TIFF

### PDF

- Image → PDF
- Multiple images → PDF
- PDF → PNG
- PDF → JPEG
- PDF → WebP

### Utility

- Drag/drop
- Batch processing
- Resize
- Quality
- Metadata policy
- Progress
- Cancellation
- Error reporting
- History
- CLI
- Basic presets

That's enough.

---

# 43. Version 2

Add:

- AVIF
- optimization presets
- advanced metadata
- PDF merge
- PDF split
- page extraction
- image comparison
- better batch workflows
- saved presets
- advanced naming
- performance dashboard

---

# 44. Version 3

Potentially:

```text
OCR
Background removal
Smart compression
AI image enhancement
Semantic image organization
```

But those should be **plugins/modules**, not mixed into the core.

---

# 45. Development Phases for the AI Coding Agent

This is extremely important.

Do **not** tell an AI coding agent:

> "Build ForgeConvert."

and let it generate 40,000 lines of code.

That's how you get architectural garbage.

Instead:

### Phase 0 — Architecture

Agent produces:

- architecture document
- ADRs
- directory structure
- domain model
- interfaces
- dependency decisions

No implementation yet.

---

### Phase 1 — Core image conversion

Implement:

```text
PNG
JPEG
WebP
```

Pipeline:

```text
Decode
 ↓
Normalize
 ↓
Encode
```

Tests first.

---

### Phase 2 — Conversion engine

Implement:

- job abstraction
- progress
- cancellation
- errors
- output naming
- atomic writes

---

### Phase 3 — Batch engine

Implement:

- queue
- worker pool
- bounded concurrency
- progress
- failure reporting

---

### Phase 4 — PDF

Implement:

```text
Images → PDF
PDF → images
```

---

### Phase 5 — Tauri UI

Build the desktop interface around the already-tested core.

---

### Phase 6 — CLI

Expose the same core.

---

### Phase 7 — Optimization

Add:

- resize
- compression
- metadata
- presets

---

### Phase 8 — Hardening

Run:

- integration tests
- malformed-file tests
- large-file tests
- concurrency tests
- cancellation tests
- permission tests
- performance benchmarks

---

# 46. AI Coding Agent Rules

This is the part I would be strict about.

Your coding agent should follow these rules:

### Rule 1

**Never invent architecture during implementation.**

If a requirement conflicts with the architecture, stop and document the conflict.

### Rule 2

**Do not introduce a dependency without justification.**

Every dependency must answer:

```text
Why is it needed?
Why this library?
What alternatives were considered?
What license does it use?
```

### Rule 3

**No duplicated business logic.**

### Rule 4

**No giant files.**

If a module becomes difficult to reason about, refactor it.

### Rule 5

**No speculative abstraction.**

Don't create:

```text
AbstractUniversalConversionFactoryProviderManager
```

for code that doesn't need it.

### Rule 6

**Tests accompany functionality.**

### Rule 7

**Don't silently change public behavior.**

### Rule 8

**Never delete tests to make the build pass.**

### Rule 9

**Never hide errors.**

### Rule 10

**Prefer boring, reliable engineering over clever code.**

This is particularly important when using autonomous coding agents.

---

# 47. The Master Prompt for Your AI Coding Agent

This is the document I would actually give the coding agent after putting the proposal into the repository.

# ForgeConvert — AI Coding Agent Master Specification

## Mission

Build ForgeConvert, a local-first, privacy-preserving desktop utility for image and PDF conversion, optimization, inspection, and batch processing.

The application must work primarily offline and must not require external conversion APIs.

The implementation must prioritize correctness, modularity, testability, performance, and maintainability.

The project will be implemented by an AI coding agent with minimal direct human intervention.

Therefore, all architectural assumptions, contracts, invariants, and acceptance criteria must be explicit.

---

# 1. Technology

Use:

- Tauri 2
- Rust
- Vue 3
- TypeScript
- Vite
- SQLite for application metadata/history/settings where persistence is necessary

Use mature native Rust-compatible libraries for image codecs and PDF processing rather than implementing image/PDF codecs from scratch.

Do not introduce an AI/LLM dependency into the core conversion pipeline.

---

# 2. Architectural Style

Use a modular layered architecture influenced by hexagonal architecture.

Required conceptual layers:

1. Presentation
2. Application
3. Domain
4. Infrastructure

Responsibilities must remain separated.

The frontend must not contain conversion business logic.

The domain must not depend directly on Tauri.

The domain must not depend directly on SQLite.

The conversion engine must be usable independently of the desktop UI.

The CLI must reuse the same application/core functionality as the desktop application.

---

# 3. Core Principle

All supported image formats must converge into a common internal representation before transformation/output.

Required conceptual pipeline:

Input File
→ Format Detection
→ Decoder
→ Canonical Image Representation
→ Transform Pipeline
→ Encoder/Document Writer
→ Temporary Output
→ Atomic Finalization

Do not implement every format pair as a separate conversion path.

---

# 4. Supported Initial Formats

Image input/output:

- PNG
- JPEG
- WebP
- BMP
- TIFF

PDF:

- PDF input
- PDF output

Initial workflows:

- image → image
- image → PDF
- multiple images → PDF
- PDF → image
- PDF page range → images

---

# 5. Core Domain Concepts

Define explicit domain types for:

- ConversionJob
- ConversionRequest
- ConversionOptions
- OutputTarget
- ImageMetadata
- ImageDimensions
- PixelFormat
- ColorSpace
- FormatDescriptor
- FormatCapabilities
- JobStatus
- JobProgress
- ConversionResult
- ConversionError
- BatchJob
- Preset

Avoid primitive obsession where strong domain types improve correctness.

---

# 6. Job Model

Every conversion must execute through a job abstraction.

Valid states:

- Queued
- Running
- Completed
- Failed
- Cancelled

State transitions must be validated.

Invalid transitions must produce structured errors.

Every job must expose:

- unique ID
- input files
- output configuration
- current state
- progress
- error information
- timestamps
- output information

---

# 7. Image Conversion

Implement the following conceptual architecture:

Decoder
→ Canonical Image Representation
→ Transform Pipeline
→ Encoder

The canonical image representation should contain enough information to support:

- dimensions
- pixel format
- color information
- alpha
- pixels
- relevant metadata

The implementation must not unnecessarily copy large image buffers.

---

# 8. Transform Pipeline

Transformations should be composable.

Initial transformations:

- resize
- quality/compression configuration
- metadata policy
- background handling where alpha must be removed
- orientation handling

Future transformations may include:

- crop
- rotate
- watermark
- sharpen
- color adjustment

Do not implement future transformations unless required.

---

# 9. Metadata Policy

Support explicit metadata behavior:

- Preserve
- Remove

Never silently remove metadata unless the format conversion inherently cannot preserve it.

The UI must clearly communicate metadata behavior when relevant.

---

# 10. JPEG Rules

JPEG does not support transparency.

When converting RGBA input to JPEG, require an explicit or sensible background compositing policy.

Default to a safe, predictable background.

Never silently pretend that transparency was preserved.

---

# 11. PDF Architecture

PDF processing must be isolated from the image conversion subsystem.

Implement separate concepts for:

- PDF document
- PDF page
- page dimensions
- page layout
- PDF rendering
- PDF writing

Image → PDF must support:

- A4
- Letter
- portrait
- landscape
- margins
- fit
- fill
- center

PDF → image must support:

- PNG
- JPEG
- WebP
- configurable DPI
- page ranges

---

# 12. Batch Processing

Implement bounded concurrency.

Never create an unbounded task per input file.

Use a worker pool or equivalent bounded execution model.

Requirements:

- configurable concurrency
- progress reporting
- cancellation
- per-file success/failure
- aggregate progress
- no unbounded memory growth

Each input should be processed independently wherever possible.

---

# 13. Memory

Do not load an entire batch into memory.

Use bounded processing.

Process:

input
→ decode
→ transform
→ encode
→ write
→ release

before moving to the next item when concurrency/resource constraints require it.

Large files must not cause uncontrolled memory growth.

---

# 14. File Writing

Never directly expose partially written output as a completed file.

Use:

temporary file
→ flush/sync as appropriate
→ successful validation where practical
→ atomic rename/finalization

Handle:

- permission errors
- missing directories
- disk-full conditions
- output collisions
- cancellation

---

# 15. Output Naming

Default behavior:

input filename
→ same basename
→ new extension

Example:

logo.png
→ logo.webp

Support customizable naming patterns in later phases.

Never silently overwrite existing files unless explicitly configured.

---

# 16. Error Handling

Use structured errors.

At minimum support:

- UnsupportedFormat
- InvalidFile
- DecodeFailed
- EncodeFailed
- PdfReadFailed
- PdfRenderFailed
- PdfWriteFailed
- PermissionDenied
- DiskFull
- OutputExists
- Cancelled
- InvalidConfiguration
- ResourceLimitExceeded

The UI should translate structured errors into human-readable messages.

Do not use arbitrary string errors as the primary internal error model.

---

# 17. Security

Treat all input files as untrusted.

Requirements:

- safe path handling
- no arbitrary command execution
- no shell invocation for normal conversion
- safe temporary directories
- cleanup temporary files
- protection against path traversal
- graceful handling of malformed files
- no execution of converted files
- avoid logging file contents

---

# 18. Privacy

The application must be local-first.

Do not upload user files.

Do not require an account.

Do not require a remote API.

Do not add telemetry by default.

Do not log sensitive file contents.

---

# 19. Frontend

Use Vue 3 + TypeScript.

The UI should remain simple.

Primary screens:

- Convert
- Batch
- PDF
- Optimize
- History
- Settings

Main conversion workflow:

1. Drop/select files
2. Detect files
3. Select output format
4. Configure options
5. Preview summary
6. Convert
7. Show progress
8. Show results

Do not build unnecessary dashboard functionality.

---

# 20. Frontend/Backend Contract

Tauri commands should expose application-level operations rather than low-level codec functions.

Bad:

convert_png_to_jpeg()

Better:

create_conversion_job()

start_job()

cancel_job()

get_job_status()

get_format_capabilities()

get_file_info()

The backend remains authoritative for validation and capability detection.

---

# 21. CLI

Create a CLI that uses the same conversion core.

Example conceptual commands:

forgeconvert convert input.png --to webp

forgeconvert batch ./images --to webp --quality 80

forgeconvert pdf image1.png image2.jpg --output document.pdf

forgeconvert render document.pdf --format png --dpi 200

forgeconvert info image.png

forgeconvert optimize logo.png --preset web

CLI behavior must be script-friendly.

Errors must return appropriate non-zero exit codes.

---

# 22. Optimization Presets

Provide presets such as:

- Web Optimized
- High Quality JPEG
- Small JPEG
- Lossless PNG
- WebP Optimized
- PDF High Quality
- PDF Small Size

Presets must be represented as configuration rather than hardcoded conversion branches.

---

# 23. Web Optimization

Provide a workflow optimized for web-development assets.

Example:

PNG
→ WebP
→ preserve dimensions
→ configurable quality
→ optional metadata removal

Display:

- original size
- output size
- percentage reduction
- dimensions
- format

Do not claim that a particular output is always "better"; show measurable properties.

---

# 24. Image Inspection

Provide file information:

- format
- dimensions
- pixel format
- color information where available
- alpha support
- metadata presence
- file size

Do not decode unnecessarily large images multiple times merely to display basic information.

---

# 25. History

Store conversion metadata in SQLite.

History should contain:

- job ID
- operation
- input filename
- output filename
- output format
- status
- timestamp
- duration
- relevant options

Do not store the actual image/PDF contents in SQLite.

---

# 26. Testing

Required test categories:

## Unit

Test:

- format detection
- option validation
- job state transitions
- output naming
- preset handling
- capability discovery

## Integration

Test:

- PNG → JPEG
- PNG → WebP
- JPEG → PNG
- JPEG → WebP
- WebP → PNG
- image → PDF
- multiple images → PDF
- PDF → PNG
- PDF → JPEG
- PDF → WebP

## Failure cases

Test:

- corrupt files
- unsupported files
- missing input
- inaccessible directories
- existing output
- cancellation
- malformed PDF
- very large files where practical

---

# 27. Test Fixtures

Maintain stable test fixtures.

Example:

tests/fixtures/

- small.png
- transparent.png
- photo.jpg
- logo.webp
- sample.bmp
- sample.tiff
- multipage.pdf
- malformed.pdf

Do not commit unnecessarily huge binary fixtures.

---

# 28. Performance

Measure:

- conversion latency
- throughput
- peak memory
- batch throughput
- PDF rendering time
- CPU utilization where practical

Benchmark:

- single file
- 10 files
- 100 files
- large image
- batch workload

Do not optimize prematurely.

Measure first.

---

# 29. Logging

Use structured logging.

Include:

- job ID
- operation
- input type
- output type
- duration
- status
- error category

Do not log:

- image contents
- PDF contents
- sensitive metadata
- arbitrary user data

---

# 30. Architecture Documentation

Create:

docs/architecture/

and:

docs/adr/

Document significant decisions.

Every ADR must include:

- Context
- Decision
- Alternatives
- Consequences

At minimum create ADRs for:

1. Tauri
2. Rust
3. Vue/TypeScript
4. local-first architecture
5. layered architecture
6. canonical image representation
7. job architecture
8. bounded worker pool
9. SQLite usage
10. PDF subsystem separation
11. shared CLI/core
12. no AI dependency in core

---

# 31. Dependency Policy

Do not add dependencies casually.

Before adding a dependency, evaluate:

- maturity
- maintenance
- license
- security
- performance
- platform support
- native dependencies
- API quality

Prefer mature libraries over custom implementations of complex standards.

Do not implement PNG/JPEG/WebP/PDF codecs from scratch.

---

# 32. Architecture Restrictions

Do NOT introduce:

- microservices
- cloud infrastructure
- Kubernetes
- remote conversion APIs
- unnecessary message brokers
- unnecessary event buses
- unnecessary abstraction layers
- AI agents inside the conversion pipeline

This is a local desktop utility.

Keep the architecture proportional to the problem.

---

# 33. Implementation Order

Implement strictly in phases.

## Phase 0

Architecture and ADRs.

Do not implement features yet.

## Phase 1

Project skeleton and domain model.

## Phase 2

PNG/JPEG/WebP conversion.

## Phase 3

Job engine.

## Phase 4

Batch processing.

## Phase 5

PDF image conversion.

## Phase 6

Tauri desktop UI.

## Phase 7

CLI.

## Phase 8

Optimization and metadata features.

## Phase 9

History and presets.

## Phase 10

Security, reliability, performance, and packaging.

Do not skip directly to Phase 10.

---

# 34. Agent Development Protocol

Before modifying code:

1. Inspect the repository.
2. Understand existing architecture.
3. Identify affected modules.
4. Read relevant tests.
5. Determine whether the requested change violates existing boundaries.
6. Implement the smallest coherent change.
7. Add/update tests.
8. Run formatting.
9. Run static checks.
10. Run tests.
11. Review the resulting diff.
12. Update documentation/ADR if architecture changed.

Never modify unrelated files without justification.

Never remove tests merely to make the build pass.

Never silently change public behavior.

Never duplicate existing functionality.

---

# 35. Definition of Done

A feature is complete only when:

- implementation exists
- tests exist
- errors are handled
- cancellation is handled where relevant
- logging is appropriate
- documentation is updated
- architecture boundaries remain intact
- formatting passes
- lint/static checks pass
- tests pass
- no obvious resource leak exists
- no unnecessary dependency was introduced

---

# 36. Final Engineering Principle

The goal is not to create the largest converter.

The goal is to create a small, reliable, local-first file-processing system with excellent architecture.

Prefer:

simple
→ explicit
→ testable
→ measurable
→ modular

over:

clever
→ abstract
→ over-engineered
→ difficult to debug

When uncertain, preserve the architectural boundaries and document the decision rather than inventing a new subsystem.

---

## 48. My honest assessment of the project

I think this is a **good project for you**, but not because "image converter" is an impressive portfolio project.

By itself:

> **"I built a PNG-to-JPEG converter."**

is weak.

You could write that in an afternoon.

The interesting project is:

> **A local-first, extensible document/image processing engine with a desktop UI, CLI, batch execution, bounded concurrency, PDF rendering, format abstraction, optimization pipelines, structured errors, and automated testing.**

That's a legitimate systems project.

And there's another reason I like it for your current learning path: it forces you to practice architecture without needing an enormous distributed system.

You'll encounter:

```text
Software Architecture
       ↓
Domain Modeling
       ↓
API Design
       ↓
File Systems
       ↓
Binary Formats
       ↓
Memory Management
       ↓
Concurrency
       ↓
Performance
       ↓
Error Handling
       ↓
Testing
       ↓
Desktop Architecture
       ↓
CLI Architecture
```

That's a very good architectural laboratory.

### One final rule I'd impose on yourself

**Don't let the coding agent make the architecture invisible to you.**

Even if the agent writes 95% of the code, you should understand:

- why the conversion pipeline is designed this way,
- why the canonical representation exists,
- why jobs exist,
- why the worker pool is bounded,
- why SQLite stores metadata rather than files,
- why PDF is separated from image codecs,
- why the UI doesn't own business logic,
- why the CLI shares the same core,
- why you didn't use microservices,
- and what happens when a 500 MB malformed PDF is dropped into the application.

If you can explain those decisions clearly, **you are learning architecture while using the coding agent rather than simply outsourcing programming to it.**

And that's exactly how I'd want you to approach this project.
