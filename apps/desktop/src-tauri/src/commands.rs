//! Tauri commands: job-level API over the shared engine (ADR 001/010).
//!
//! Every command is synchronous-over-`spawn_blocking` friendly (Tauri
//! commands are async; blocking codec/SQLite work stays off the async
//! runtime via the engine's own design — single calls run inline, batch
//! progress streams through events in a later iteration).

use std::path::PathBuf;

use forge_core::{
    ConversionOptions, ConversionRequest, FileSystem as _, ForgeError, ImageDecoder as _,
    ImageFormat, JobStatus, MetadataPolicy, NeverCancel, OutputTarget, PageRange, PdfWriter as _,
    Preset,
};
use forge_engine::{EngineDeps, NullSink, Orchestrator, StdFileSystem};
use forge_image::{FitWithinStep, ForgeImageDecoder, ForgeImageEncoder};
use forge_pdf::ForgePdfWriter;
use serde::{Deserialize, Serialize};

/// JSON-safe error shape (variant name + message; never internals).
#[derive(Debug, Serialize)]
pub(crate) struct CommandError {
    kind: String,
    message: String,
}

impl From<ForgeError> for CommandError {
    fn from(error: ForgeError) -> Self {
        let kind = match error {
            ForgeError::UnsupportedFormat(_) => "UnsupportedFormat",
            ForgeError::InvalidFile(_) => "InvalidFile",
            ForgeError::DecodeFailed(_) => "DecodeFailed",
            ForgeError::EncodeFailed(_) => "EncodeFailed",
            ForgeError::PdfReadFailed(_) => "PdfReadFailed",
            ForgeError::PdfRenderFailed(_) => "PdfRenderFailed",
            ForgeError::PdfWriteFailed(_) => "PdfWriteFailed",
            ForgeError::PermissionDenied(_) => "PermissionDenied",
            ForgeError::DiskFull(_) => "DiskFull",
            ForgeError::OutputExists(_) => "OutputExists",
            ForgeError::Cancelled => "Cancelled",
            ForgeError::InvalidConfiguration(_) => "InvalidConfiguration",
            ForgeError::ResourceLimitExceeded(_) => "ResourceLimitExceeded",
            ForgeError::InvalidTransition { .. } => "InvalidTransition",
            ForgeError::Unsupported { .. } => "Unsupported",
            _ => "Unknown",
        };
        Self {
            kind: kind.to_string(),
            message: error.to_string(),
        }
    }
}

type CommandResult<T> = Result<T, CommandError>;

/// One format descriptor for the UI's format pickers.
#[derive(Debug, Serialize)]
pub(crate) struct FormatInfo {
    id: String,
    name: String,
    extensions: Vec<String>,
    mime_types: Vec<String>,
    can_decode: bool,
    can_encode: bool,
    supports_alpha: bool,
}

/// `get_format_capabilities` — what can convert to what (backend authoritative).
#[tauri::command]
pub fn get_format_capabilities() -> Vec<FormatInfo> {
    ImageFormat::all()
        .iter()
        .map(|format| {
            let descriptor = format.descriptor();
            let id = match format {
                ImageFormat::Jpeg => "jpg",
                _ => format.extension(),
            };
            FormatInfo {
                id: id.to_string(),
                name: descriptor.name.to_string(),
                extensions: descriptor
                    .extensions
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                mime_types: descriptor
                    .mime_types
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                can_decode: descriptor.can_decode,
                can_encode: descriptor.can_encode,
                supports_alpha: descriptor.supports_alpha,
            }
        })
        .collect()
}

/// File inspection for the UI panel (format, dims, alpha, size).
#[derive(Debug, Serialize)]
pub(crate) struct FileInfo {
    name: String,
    format: String,
    mime_type: String,
    width: Option<u32>,
    height: Option<u32>,
    pixel: Option<String>,
    alpha: bool,
    size_bytes: u64,
}

#[tauri::command]
pub fn get_file_info(path: String) -> CommandResult<FileInfo> {
    let path = PathBuf::from(&path);
    let fs = StdFileSystem;
    let bytes = fs.read(&path).map_err(CommandError::from)?;
    let format =
        forge_core::detect_input_format(&path, Some(&bytes)).map_err(CommandError::from)?;
    let size_bytes = std::fs::metadata(&path)
        .map(|m| m.len())
        .unwrap_or(bytes.len() as u64);
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    if format == ImageFormat::Pdf {
        let descriptor = format.descriptor();
        return Ok(FileInfo {
            name,
            format: descriptor.name.to_string(),
            mime_type: descriptor.mime_types.join(","),
            width: None,
            height: None,
            pixel: None,
            alpha: false,
            size_bytes,
        });
    }
    let decoder = ForgeImageDecoder;
    let image = decoder
        .decode(&bytes, Some(format))
        .map_err(CommandError::from)?;
    let descriptor = format.descriptor();
    Ok(FileInfo {
        name,
        format: descriptor.name.to_string(),
        mime_type: descriptor.mime_types.join(","),
        width: Some(image.dimensions.width),
        height: Some(image.dimensions.height),
        pixel: Some(format!("{:?}", image.pixel_format)),
        alpha: image.has_alpha,
        size_bytes,
    })
}

/// Convert options from the UI (mirrors `ConversionOptions`; validated in Rust).
#[derive(Debug, Deserialize)]
pub(crate) struct ConvertArgs {
    inputs: Vec<String>,
    to: String,
    output_dir: Option<String>,
    quality: Option<u8>,
    max_width: Option<u32>,
    max_height: Option<u32>,
    strip_metadata: Option<bool>,
    on_collision: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ConvertDone {
    outputs: Vec<String>,
    skipped: Vec<String>,
    failures: Vec<String>,
}

fn parse_format(raw: &str) -> Result<ImageFormat, CommandError> {
    ImageFormat::from_extension(raw).ok_or_else(|| {
        ForgeError::UnsupportedFormat(format!("unknown target format {raw:?}")).into()
    })
}

fn parse_collision(raw: Option<&str>) -> Result<forge_core::CollisionPolicy, CommandError> {
    use forge_core::CollisionPolicy;
    match raw.unwrap_or("rename").to_ascii_lowercase().as_str() {
        "rename" | "rename-auto" | "auto" => Ok(CollisionPolicy::RenameAuto),
        "replace" | "overwrite" => Ok(CollisionPolicy::Replace),
        "skip" => Ok(CollisionPolicy::Skip),
        "fail" | "error" => Ok(CollisionPolicy::Fail),
        other => Err(ForgeError::InvalidConfiguration(format!(
            "unknown collision policy {other:?}"
        ))
        .into()),
    }
}

fn engine() -> (
    StdFileSystem,
    ForgeImageDecoder,
    ForgeImageEncoder,
    FitWithinStep,
) {
    (
        StdFileSystem,
        ForgeImageDecoder,
        ForgeImageEncoder,
        FitWithinStep,
    )
}

/// `convert_image` — convert files via the shared orchestrator.
#[tauri::command]
pub fn convert_image(args: ConvertArgs) -> CommandResult<ConvertDone> {
    let target = parse_format(&args.to)?;
    if target == ImageFormat::Pdf {
        return Err(ForgeError::InvalidConfiguration(
            "use convert_images_to_pdf for PDF output".to_string(),
        )
        .into());
    }
    let max_dimensions = match (args.max_width, args.max_height) {
        (None, None) => None,
        (w, h) => Some(
            forge_core::ImageDimensions::new(w.unwrap_or(u32::MAX), h.unwrap_or(u32::MAX))
                .map_err(CommandError::from)?,
        ),
    };
    let options = ConversionOptions {
        quality: args.quality.unwrap_or(80),
        max_dimensions,
        metadata: if args.strip_metadata.unwrap_or(false) {
            MetadataPolicy::Remove
        } else {
            MetadataPolicy::Preserve
        },
        background: Default::default(),
        on_collision: parse_collision(args.on_collision.as_deref())?,
    }
    .validated()
    .map_err(CommandError::from)?;
    let inputs: Vec<PathBuf> = args.inputs.iter().map(PathBuf::from).collect();
    let output = match args.output_dir {
        Some(dir) => OutputTarget::Directory(PathBuf::from(dir)),
        None => {
            let first = inputs.first().ok_or_else(|| {
                CommandError::from(ForgeError::InvalidConfiguration(
                    "at least one input is required".to_string(),
                ))
            })?;
            OutputTarget::Directory(
                first
                    .parent()
                    .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
            )
        }
    };
    let request = ConversionRequest {
        inputs,
        output_format: target,
        output,
        options,
    }
    .validated()
    .map_err(CommandError::from)?;
    let (fs, decoder, encoder, fit) = engine();
    let transforms: [&dyn forge_core::TransformStep; 1] = [&fit];
    let orchestrator = Orchestrator::new(EngineDeps {
        decoder: &decoder,
        encoder: &encoder,
        transforms: &transforms,
        fs: &fs,
    });
    let result = orchestrator
        .run(request, &NullSink, &NeverCancel)
        .map_err(CommandError::from)?;
    Ok(ConvertDone {
        outputs: result
            .outputs
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        skipped: result
            .skipped
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        failures: result.failures.clone(),
    })
}

/// `convert_images_to_pdf` — pack images, one page each.
#[derive(Debug, Deserialize)]
pub(crate) struct ImagesToPdfArgs {
    inputs: Vec<String>,
    output: String,
    page: Option<String>,
    landscape: Option<bool>,
}

#[tauri::command]
pub fn convert_images_to_pdf(args: ImagesToPdfArgs) -> CommandResult<String> {
    use forge_core::{Orientation, PageSizeMm, PdfWriteSpec};
    if args.inputs.is_empty() {
        return Err(
            ForgeError::InvalidConfiguration("at least one input is required".to_string()).into(),
        );
    }
    let fs = StdFileSystem;
    let decoder = ForgeImageDecoder;
    let mut images = Vec::with_capacity(args.inputs.len());
    for input in &args.inputs {
        let path = PathBuf::from(input);
        let bytes = fs.read(&path).map_err(CommandError::from)?;
        let format =
            forge_core::detect_input_format(&path, Some(&bytes)).map_err(CommandError::from)?;
        images.push(
            decoder
                .decode(&bytes, Some(format))
                .map_err(CommandError::from)?,
        );
    }
    let spec = PdfWriteSpec {
        page: match args
            .page
            .as_deref()
            .unwrap_or("a4")
            .to_ascii_lowercase()
            .as_str()
        {
            "letter" => PageSizeMm::LETTER,
            _ => PageSizeMm::A4,
        },
        orientation: if args.landscape.unwrap_or(false) {
            Orientation::Landscape
        } else {
            Orientation::Portrait
        },
        ..Default::default()
    };
    let writer = ForgePdfWriter;
    let bytes = writer
        .write_images(&images, &spec)
        .map_err(CommandError::from)?;
    let output = PathBuf::from(&args.output);
    fs.write_atomic(&output, &bytes)
        .map_err(CommandError::from)?;
    Ok(output.display().to_string())
}

/// `list_presets` — built-in preset catalogue for the UI pickers.
#[derive(Debug, Serialize)]
pub(crate) struct PresetInfo {
    key: String,
    name: String,
    format: String,
    quality: u8,
}

#[tauri::command]
pub fn list_presets() -> Vec<PresetInfo> {
    Preset::builtins()
        .iter()
        .map(|preset| PresetInfo {
            key: preset.key(),
            name: preset.name.clone(),
            format: preset.output_format.extension().to_string(),
            quality: preset.options.quality,
        })
        .collect()
}

/// `get_history` — recent conversions (metadata only).
#[derive(Debug, Serialize)]
pub(crate) struct HistoryRow {
    job_id: String,
    operation: String,
    input: String,
    output: String,
    format: String,
    status: String,
    duration_ms: u64,
}

#[tauri::command]
pub fn get_history(limit: Option<usize>) -> CommandResult<Vec<HistoryRow>> {
    use forge_core::HistoryStore as _;
    let db = open_history().map_err(CommandError::from)?;
    let entries = db.recent(limit.unwrap_or(20)).map_err(CommandError::from)?;
    Ok(entries
        .into_iter()
        .map(|entry| HistoryRow {
            job_id: entry.job_id.0,
            operation: entry.operation,
            input: entry.input_name,
            output: entry.output_name,
            format: entry.output_format.extension().to_string(),
            status: entry.status.name().to_string(),
            duration_ms: entry.duration_ms,
        })
        .collect())
}

/// History DB location mirrors the CLI (shared local history).
fn open_history() -> Result<forge_store::HistoryDb, ForgeError> {
    let path = history_path();
    forge_store::HistoryDb::open(&path).or_else(|_| forge_store::HistoryDb::in_memory())
}

fn history_path() -> PathBuf {
    for key in ["XDG_DATA_HOME", "LOCALAPPDATA", "APPDATA"] {
        if let Ok(dir) = std::env::var(key) {
            if !dir.is_empty() {
                return PathBuf::from(dir).join("forgeconvert").join("history.db");
            }
        }
    }
    PathBuf::from(".forgeconvert").join("history.db")
}

/// Re-exported for a smoke test asserting job-state strings.
#[allow(dead_code)]
fn status_name(status: JobStatus) -> &'static str {
    status.name()
}

/// Re-exported for a smoke test asserting page-range parsing.
#[allow(dead_code)]
fn parse_pages(spec: &str) -> Result<PageRange, ForgeError> {
    PageRange::parse(spec)
}

use std::path::Path;
