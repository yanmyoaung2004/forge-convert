//! Hexagonal ports: traits the engine depends on, adapters implement.
//!
//! Boundary rules:
//! - Engine owns orchestration; adapters own I/O and codecs.
//! - Ports take/return domain types + `ForgeError` only.
//! - All ports are object-safe (`Send + Sync`) for the worker pool.
use std::path::Path;

use crate::domain::canonical::CanonicalImage;
use crate::domain::format::ImageFormat;
use crate::domain::job::{ConversionRequest, ConversionResult, JobId, JobProgress, JobStatus};
use crate::domain::options::ConversionOptions;
use crate::error::{ForgeError, Result};

/// Decode bytes → [`CanonicalImage`] (ADR 005).
pub trait ImageDecoder: Send + Sync {
    /// Formats this decoder accepts.
    fn supported_inputs(&self) -> &'static [ImageFormat];
    /// Decode; `hint` may carry the source format (sniffed earlier).
    fn decode(&self, bytes: &[u8], hint: Option<ImageFormat>) -> Result<CanonicalImage>;
}

/// Encode [`CanonicalImage`] → bytes.
pub trait ImageEncoder: Send + Sync {
    /// Formats this encoder produces.
    fn supported_outputs(&self) -> &'static [ImageFormat];
    /// Encode with options (quality, background flattening, …).
    fn encode(
        &self,
        image: &CanonicalImage,
        target: ImageFormat,
        options: &ConversionOptions,
    ) -> Result<Vec<u8>>;
}

/// One composable transform step (resize, flatten, orient, …).
pub trait TransformStep: Send + Sync {
    /// Stable name for logging (`"flatten-alpha"`, `"fit-within"`).
    fn name(&self) -> &'static str;
    /// Pure transform; returns the (possibly new) image.
    fn apply(&self, image: CanonicalImage, options: &ConversionOptions) -> Result<CanonicalImage>;
}

/// Image(s) → PDF bytes (ADR 009).
pub trait PdfWriter: Send + Sync {
    /// One page per image; layout/fit handled by the caller spec.
    fn write_images(&self, images: &[CanonicalImage], spec: &PdfWriteSpec) -> Result<Vec<u8>>;
}

/// Page size for PDF output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSizeMm {
    pub width_mm: f64,
    pub height_mm: f64,
}

impl PageSizeMm {
    /// A4: 210 × 297 mm.
    pub const A4: Self = Self {
        width_mm: 210.0,
        height_mm: 297.0,
    };
    /// US Letter: 8.5 × 11 in = 215.9 × 279.4 mm.
    pub const LETTER: Self = Self {
        width_mm: 215.9,
        height_mm: 279.4,
    };
}

/// Image fit inside a PDF page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageFit {
    /// Scale to fit, preserve aspect, centered.
    #[default]
    Fit,
    /// Scale to fill (crop overflow — adapters center-crop).
    Fill,
    /// No scaling (dpi decides physical size).
    None,
}

/// PDF page orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

/// Layout spec for [`PdfWriter::write_images`].
#[derive(Debug, Clone, PartialEq)]
pub struct PdfWriteSpec {
    pub page: PageSizeMm,
    pub orientation: Orientation,
    pub margins_mm: f64,
    pub fit: PageFit,
    pub dpi: f64,
}

impl Default for PdfWriteSpec {
    fn default() -> Self {
        Self {
            page: PageSizeMm::A4,
            orientation: Orientation::Portrait,
            margins_mm: 10.0,
            fit: PageFit::Fit,
            dpi: 300.0,
        }
    }
}

/// PDF → images. Default build has NO renderer: the stub returns
/// [`ForgeError::Unsupported`] (ADR 009). Qualified backends arrive
/// behind cargo features later.
pub trait PdfRenderer: Send + Sync {
    /// Render 1-based `pages` at `dpi` into canonical images.
    fn render(&self, pdf_bytes: &[u8], pages: &[u32], dpi: u16) -> Result<Vec<CanonicalImage>>;
}

/// PDF page ops: split + page count (v0.3.0 plan slice 0).
/// Implemented in `forge-pdf` over `lopdf` (ADR 009 foresaw this).
pub trait PdfSplitter: Send + Sync {
    /// Total 1-based page count of `pdf_bytes`.
    fn page_count(&self, pdf_bytes: &[u8]) -> Result<u32>;
    /// Keep `range.pages` (1-based, validated `<= total`), return new PDF bytes.
    fn split(&self, pdf_bytes: &[u8], range: &crate::domain::PageRange) -> Result<Vec<u8>>;
}

/// PDF → Word (.docx) export (v0.3.0 plan slice 0).
/// Implemented in `forge-pdf`: text extraction + `docx-rs` writer.
pub trait PdfToDocx: Send + Sync {
    /// Extract text (all pages when `range` is `None`) into .docx bytes.
    fn convert(
        &self,
        pdf_bytes: &[u8],
        range: Option<&crate::domain::PageRange>,
    ) -> Result<Vec<u8>>;
}

/// PDF merge: concatenate pages of several PDFs in order (v0.4.0).
pub trait PdfMerger: Send + Sync {
    /// Merge `pdfs` (each a full PDF) into one PDF, pages in input order.
    fn merge(&self, pdfs: &[&[u8]]) -> Result<Vec<u8>>;
}

/// PDF size compression level (v0.4.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfCompressLevel {
    /// Drop orphaned objects only (always safe, modest savings).
    Light,
    /// Orphans + recompress content streams (better savings, slower).
    #[default]
    Balanced,
}

/// PDF compress: shrink file size without changing pages (v0.4.0).
pub trait PdfCompressor: Send + Sync {
    /// Compress `pdf_bytes` at `level`, return new PDF bytes.
    fn compress(&self, pdf_bytes: &[u8], level: PdfCompressLevel) -> Result<Vec<u8>>;
}
/// Persist + query job records (engine-tested via in-memory fake;
/// SQLite adapter lands in Phase 9 behind this port).
pub trait JobRepository: Send + Sync {
    fn save(&self, snapshot: JobSnapshot) -> Result<()>;
    fn get(&self, id: &JobId) -> Result<Option<JobSnapshot>>;
    fn list_recent(&self, limit: usize) -> Result<Vec<JobSnapshot>>;
}

/// Serializable job state for stores/history UI.
#[derive(Debug, Clone)]
pub struct JobSnapshot {
    pub id: JobId,
    pub request_summary: String,
    pub status: JobStatus,
    pub progress: JobProgress,
    pub error: Option<String>,
}

/// Filesystem boundary: all engine file access goes through here
/// (temp files, atomic renames, cleanup — never `std::fs` directly).
pub trait FileSystem: Send + Sync {
    /// Read a whole file (bounded by caller limits).
    fn read(&self, path: &Path) -> Result<Vec<u8>>;
    /// Write-temp → flush/sync → validate → atomic rename.
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<()>;
    /// Best-effort temp cleanup.
    fn remove(&self, path: &Path) -> Result<()>;
}

/// History/settings store (SQLite in Phase 9; spec §25 fields).
pub trait HistoryStore: Send + Sync {
    fn record(&self, entry: HistoryEntry) -> Result<()>;
    fn recent(&self, limit: usize) -> Result<Vec<HistoryEntry>>;
}

/// One history row (metadata only — never file bytes).
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub job_id: JobId,
    pub operation: String,
    pub input_name: String,
    pub output_name: String,
    pub output_format: ImageFormat,
    pub status: JobStatus,
    pub duration_ms: u64,
    pub options_json: String,
}

/// Progress/cancellation events for UI/CLI subscribers.
pub trait JobEventSink: Send + Sync {
    fn progress(&self, id: &JobId, progress: &JobProgress);
    fn finished(&self, id: &JobId, result: &ConversionResult);
    fn failed(&self, id: &JobId, error: &ForgeError);
}

/// Clock abstraction (deterministic tests; system clock in prod).
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

/// Engine entry point contract (implemented in `forge-engine`, Phase 3+).
pub trait ConversionEngine: Send + Sync {
    /// Validate → run one request → result (progress via `sink`).
    fn convert(
        &self,
        request: ConversionRequest,
        sink: &dyn JobEventSink,
        cancel: &dyn CancelToken,
    ) -> Result<ConversionResult>;
}

/// Cooperative cancellation checked between pipeline stages.
pub trait CancelToken: Send + Sync {
    fn is_cancelled(&self) -> bool;
    fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(ForgeError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Never-cancels token (tests, one-shot CLI runs).
#[derive(Debug, Default)]
pub struct NeverCancel;

impl CancelToken for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}
