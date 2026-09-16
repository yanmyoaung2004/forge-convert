// forge-core: domain types + ports + structured errors.
// Hexagonal rule: this crate depends on NOTHING internal, no Tauri,
// no SQLite, no codec crates.
//! ForgeConvert core domain: types, ports, and structured errors.
//!
//! Layer: domain + ports (hexagonal). No internal, Tauri, SQLite,
//! or codec dependencies allowed here.

pub mod domain;
pub mod error;
pub mod ports;
pub use domain::{
    BackgroundPolicy, CanonicalImage, CollisionPolicy, ConversionJob, ConversionOptions,
    ConversionRequest, ConversionResult, FormatCapabilities, FormatDescriptor, ImageDimensions,
    ImageFormat, ImageMetadata, JobId, JobProgress, JobStatus, MetadataPolicy, OutputTarget,
    PageRange, Preset, canonical_stem_of, default_output_extension, detect_format,
    detect_input_format, output_path_for,
};
pub use domain::{ColorSpace, PixelFormat};
pub use error::{ForgeError, Result};
pub use ports::{
    Clock, FileSystem, HistoryStore, ImageDecoder, ImageEncoder, JobEventSink, JobRepository,
    PdfRenderer, PdfWriter, TransformStep,
};
