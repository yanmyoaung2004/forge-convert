//! Jobs: identity, lifecycle, naming, requests, results.
//!
//! Every operation runs as a job (spec §10); transitions are validated
//! (invalid → [`ForgeError::InvalidTransition`]).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::domain::format::{ImageFormat, detect_format};
use crate::domain::options::ConversionOptions;
use crate::domain::output::OutputTarget;
use crate::error::{ForgeError, Result};

/// Opaque job identifier (UUID-style string).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JobId(pub String);

impl JobId {
    /// Generate a unique id from time + process + counter (no extra deps).
    #[must_use]
    pub fn generate() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(format!("{nanos:032x}-{count:016x}"))
    }
}

/// Job lifecycle states (spec §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatus {
    /// Human name for logs/UI.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Running => "Running",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }

    /// Terminal states accept no further transitions.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled
        )
    }

    /// Validate `from → to`; returns `to` or [`ForgeError::InvalidTransition`].
    pub fn transition(from: Self, to: Self) -> Result<Self> {
        let allowed = matches!(
            (from, to),
            (Self::Queued, Self::Running)
                | (Self::Queued, Self::Cancelled)
                | (Self::Running, Self::Completed)
                | (Self::Running, Self::Failed)
                | (Self::Running, Self::Cancelled)
        );
        if allowed {
            Ok(to)
        } else {
            Err(ForgeError::InvalidTransition {
                from: from.name(),
                to: to.name(),
            })
        }
    }
}

/// Fraction-complete progress (0.0–1.0 + optional message).
#[derive(Debug, Clone, PartialEq)]
pub struct JobProgress {
    /// Completed fraction, clamped to 0.0–1.0.
    pub fraction: f32,
    /// Optional stage message (`"encoding"`).
    pub message: Option<String>,
}

impl JobProgress {
    /// Clamp fraction into range.
    #[must_use]
    pub fn new(fraction: f32, message: Option<String>) -> Self {
        Self {
            fraction: fraction.clamp(0.0, 1.0),
            message,
        }
    }
}

/// What to convert, where, and how — validated before a job starts.
#[derive(Debug, Clone)]
pub struct ConversionRequest {
    /// Existing input files.
    pub inputs: Vec<PathBuf>,
    /// Target format.
    pub output_format: ImageFormat,
    /// Destination (directory or explicit file for single-input jobs).
    pub output: OutputTarget,
    /// Validated knobs.
    pub options: ConversionOptions,
}

impl ConversionRequest {
    /// At least one input; single input for explicit-file outputs.
    pub fn validated(self) -> Result<Self> {
        if self.inputs.is_empty() {
            return Err(ForgeError::InvalidConfiguration(
                "at least one input file is required".to_string(),
            ));
        }
        if matches!(self.output, OutputTarget::File(_)) && self.inputs.len() > 1 {
            return Err(ForgeError::InvalidConfiguration(
                "explicit file output needs exactly one input".to_string(),
            ));
        }
        Ok(Self {
            options: self.options.validated()?,
            ..self
        })
    }
}

/// A tracked conversion operation.
#[derive(Debug, Clone)]
pub struct ConversionJob {
    pub id: JobId,
    pub request: ConversionRequest,
    pub status: JobStatus,
    pub progress: JobProgress,
    pub created_at: SystemTime,
    pub result: Option<ConversionResult>,
    pub error: Option<String>,
}

impl ConversionJob {
    /// New job in `Queued` state.
    #[must_use]
    pub fn new(request: ConversionRequest) -> Self {
        Self {
            id: JobId::generate(),
            request,
            status: JobStatus::Queued,
            progress: JobProgress::new(0.0, None),
            created_at: SystemTime::now(),
            result: None,
            error: None,
        }
    }

    /// Apply a validated transition.
    pub fn advance(&mut self, to: JobStatus) -> Result<()> {
        self.status = JobStatus::transition(self.status, to)?;
        Ok(())
    }
}

/// Outcome of a finished job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionResult {
    /// Written output files.
    pub outputs: Vec<PathBuf>,
    /// Per-input failures (batch keeps going; never "something went wrong").
    pub failures: Vec<String>,
    /// Total wall time.
    pub duration: std::time::Duration,
}

/// File-stem of `path` (`logo.png` → `logo`; `archive.tar.png` → `archive.tar`).
/// Empty stems fall back to `"output"`.
#[must_use]
pub fn canonical_stem_of(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("output")
        .to_string()
}

/// Extension for `format` (`Jpeg` → `"jpg"`).
#[must_use]
pub fn default_output_extension(format: ImageFormat) -> &'static str {
    format.extension()
}

/// Default output path: input dir (or `output_dir` when given),
/// same basename + new extension (spec §15: `logo.png → logo.webp`).
#[must_use]
pub fn output_path_for(
    input: &Path,
    format: ImageFormat,
    output_dir: Option<&Path>,
) -> PathBuf {
    let stem = canonical_stem_of(input);
    let file = format!("{stem}.{ext}", ext = default_output_extension(format));
    match output_dir {
        Some(dir) => dir.join(file),
        None => input.with_file_name(file),
    }
}

/// Detect format by sniffing bytes first, falling back to the extension.
/// Sniffing wins when both exist (content truth beats name truth).
pub fn detect_input_format(path: &Path, bytes: Option<&[u8]>) -> Result<ImageFormat> {
    if let Some(bytes) = bytes {
        if let Ok(format) = detect_format(bytes) {
            return Ok(format);
        }
    }
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(ImageFormat::from_extension)
        .ok_or_else(|| {
            ForgeError::UnsupportedFormat(format!(
                "cannot determine format of {}",
                path.display()
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ConversionRequest {
        ConversionRequest {
            inputs: vec![PathBuf::from("logo.png")],
            output_format: ImageFormat::Webp,
            output: OutputTarget::Directory(PathBuf::from("/out")),
            options: ConversionOptions::default(),
        }
        .validated()
        .unwrap()
    }

    #[test]
    fn test_valid_transitions() {
        use JobStatus::{Cancelled, Completed, Failed, Queued, Running};
        assert_eq!(JobStatus::transition(Queued, Running), Ok(Running));
        assert_eq!(JobStatus::transition(Queued, Cancelled), Ok(Cancelled));
        assert_eq!(JobStatus::transition(Running, Completed), Ok(Completed));
        assert_eq!(JobStatus::transition(Running, Failed), Ok(Failed));
        assert_eq!(JobStatus::transition(Running, Cancelled), Ok(Cancelled));
    }

    #[test]
    fn test_invalid_transitions_rejected() {
        use JobStatus::{Cancelled, Completed, Failed, Queued, Running};
        for (from, to) in [
            (Queued, Completed),
            (Queued, Failed),
            (Running, Queued),
            (Completed, Running),
            (Failed, Cancelled),
            (Cancelled, Completed),
            (Completed, Completed),
        ] {
            assert!(
                matches!(
                    JobStatus::transition(from, to),
                    Err(ForgeError::InvalidTransition { .. })
                ),
                "{from:?} → {to:?}"
            );
        }
    }

    #[test]
    fn test_job_advance_and_ids_unique() {
        let mut job = ConversionJob::new(request());
        assert_eq!(job.status, JobStatus::Queued);
        job.advance(JobStatus::Running).unwrap();
        job.advance(JobStatus::Completed).unwrap();
        assert!(job.status.is_terminal());
        assert!(job.advance(JobStatus::Failed).is_err());

        let other = ConversionJob::new(request());
        assert_ne!(job.id, other.id);
    }

    #[test]
    fn test_output_naming_rule() {
        assert_eq!(
            output_path_for(Path::new("/img/logo.png"), ImageFormat::Webp, None),
            PathBuf::from("/img/logo.webp")
        );
        assert_eq!(
            output_path_for(
                Path::new("/img/IMG_001.png"),
                ImageFormat::Jpeg,
                Some(Path::new("/out"))
            ),
            PathBuf::from("/out/IMG_001.jpg")
        );
        assert_eq!(canonical_stem_of(Path::new("no-ext")), "no-ext");
    }

    #[test]
    fn test_request_validation() {
        let empty = ConversionRequest {
            inputs: vec![],
            ..request()
        };
        assert!(empty.validated().is_err());

        let multi_to_file = ConversionRequest {
            inputs: vec![PathBuf::from("a.png"), PathBuf::from("b.png")],
            output: OutputTarget::File(PathBuf::from("/out/x.webp")),
            ..request()
        };
        assert!(multi_to_file.validated().is_err());
    }

    #[test]
    fn test_detect_prefers_bytes_over_extension() {
        let png_bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        // Misleading .jpg name, PNG content → PNG wins.
        assert_eq!(
            detect_input_format(Path::new("photo.jpg"), Some(&png_bytes)),
            Ok(ImageFormat::Png)
        );
        // No bytes → extension fallback; unknown → UnsupportedFormat.
        assert_eq!(
            detect_input_format(Path::new("scan.tiff"), None),
            Ok(ImageFormat::Tiff)
        );
        assert!(matches!(
            detect_input_format(Path::new("file.unknownext"), None),
            Err(ForgeError::UnsupportedFormat(_))
        ));
    }
}
