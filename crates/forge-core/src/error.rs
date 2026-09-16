//! Structured errors (spec §16 + Master §16, plus
//! `InvalidTransition` and `Unsupported`).
//!
//! The UI/CLI translate variants into human messages and exit codes;
//! the core never returns ad-hoc strings.

use std::path::PathBuf;
use thiserror::Error;

/// Crate result alias (error type defaulted; override `E` for precise errors).
pub type Result<T, E = ForgeError> = std::result::Result<T, E>;
/// All failures in ForgeConvert. Match on variants, never on message text.
/// `PartialEq` is derived for test assertions (`assert_eq!` on `Result`s);
/// production code matches variants with `matches!`.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ForgeError {
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("invalid file: {0}")]
    InvalidFile(String),

    #[error("decode failed: {0}")]
    DecodeFailed(String),

    #[error("encode failed: {0}")]
    EncodeFailed(String),

    #[error("PDF read failed: {0}")]
    PdfReadFailed(String),

    #[error("PDF render failed: {0}")]
    PdfRenderFailed(String),

    #[error("PDF write failed: {0}")]
    PdfWriteFailed(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("disk full writing: {0}")]
    DiskFull(PathBuf),

    #[error("output already exists: {0}")]
    OutputExists(PathBuf),

    #[error("operation cancelled")]
    Cancelled,

    #[error("invalid configuration: {0}")]
    InvalidConfiguration(String),

    #[error("resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),

    #[error("invalid job transition from {from} to {to}")]
    InvalidTransition { from: &'static str, to: &'static str },

    #[error("unsupported capability {capability}: {hint}")]
    Unsupported {
        capability: &'static str,
        hint: &'static str,
    },
}
