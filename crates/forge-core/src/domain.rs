//! Domain model (spec §§7–11, Master Spec §§5–11).
//!
//! Pure types + validation. No I/O, no codecs, no threads here.

pub mod canonical;
pub mod format;
pub mod job;
pub mod options;
pub mod output;
pub mod preset;

pub use canonical::CanonicalImage;
pub use format::{
    ColorSpace, FormatCapabilities, FormatDescriptor, ImageFormat, PixelFormat, detect_format,
};
pub use job::{
    ConversionJob, ConversionRequest, ConversionResult, JobId, JobProgress, JobStatus,
    canonical_stem_of, default_output_extension, detect_input_format, output_path_for,
};
pub use options::{
    BackgroundPolicy, CollisionPolicy, ConversionOptions, ImageDimensions, ImageMetadata,
    MetadataPolicy, PageRange,
};
pub use output::OutputTarget;
pub use preset::Preset;
