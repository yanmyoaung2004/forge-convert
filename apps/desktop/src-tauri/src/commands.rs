//! Tauri commands: job-level API over the shared engine (ADR 001/010).
//!
//! Every command is synchronous-over-`spawn_blocking` friendly (Tauri
//! commands are async; blocking codec/SQLite work stays off the async
//! runtime via the engine's own design — single calls run inline, batch
//! progress streams through events in a later iteration).

use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

use forge_core::{
    ConversionOptions, ConversionRequest, FileSystem as _, ForgeError, ImageDecoder as _,
    ImageFormat, JobStatus, MetadataPolicy, OutputTarget, PageRange, PdfWriter as _, Preset,
};
use forge_engine::{CancelRegistry, EngineDeps, NullSink, Orchestrator, StdFileSystem};
use forge_image::{ForgeImageDecoder, ForgeImageEncoder, ResizeStep};
use forge_pdf::ForgePdfWriter;
use serde::{Deserialize, Serialize};

/// Process-wide cancel registry: `convert_image` registers each run,
/// `cancel_convert` aborts it. Keyed by an internal id (single-flight UI
/// keeps at most one live conversion; stale entries are removed on settle).
static CANCELS: LazyLock<CancelRegistry> = LazyLock::new(CancelRegistry::new);

/// The single in-flight conversion id (None when idle).
static CURRENT_JOB: LazyLock<Mutex<Option<forge_core::JobId>>> = LazyLock::new(|| Mutex::new(None));

/// `CancelToken` adapter over a shared `CancelFlag`.
struct Cancellable(Arc<forge_engine::CancelFlag>);

impl forge_core::CancelToken for Cancellable {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

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
    width: Option<u32>,
    height: Option<u32>,
    fit: Option<String>,
    fill: Option<String>,
    filter: Option<String>,
    upscale: Option<bool>,
    png_level: Option<u8>,
    webp_lossless: Option<bool>,
    strip_metadata: Option<bool>,
    on_collision: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ConvertDone {
    outputs: Vec<String>,
    skipped: Vec<String>,
    failures: Vec<String>,
    /// Parallel to `outputs`: input bytes, output bytes per file.
    /// UIs compute % saved without a second round-trip.
    sizes: Vec<FileSizes>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FileSizes {
    output: String,
    input_bytes: u64,
    output_bytes: u64,
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

fn parse_filter(raw: Option<&str>) -> Result<forge_core::ResizeFilter, CommandError> {
    use forge_core::ResizeFilter;
    match raw.unwrap_or("lanczos3").to_ascii_lowercase().as_str() {
        "lanczos3" | "lanczos" => Ok(ResizeFilter::Lanczos3),
        "catmullrom" | "catmull-rom" | "bicubic" => Ok(ResizeFilter::CatmullRom),
        "gaussian" => Ok(ResizeFilter::Gaussian),
        "nearest" => Ok(ResizeFilter::Nearest),
        other => Err(ForgeError::InvalidConfiguration(format!("unknown filter {other:?}")).into()),
    }
}

fn parse_box(raw: &str) -> Result<forge_core::ImageDimensions, CommandError> {
    let (w, h) = raw.split_once(['x', 'X']).ok_or_else(|| {
        CommandError::from(ForgeError::InvalidConfiguration(format!(
            "expected WIDTHxHEIGHT, got {raw:?}"
        )))
    })?;
    let width: u32 = w.trim().parse().map_err(|_| {
        CommandError::from(ForgeError::InvalidConfiguration(format!(
            "bad width in {raw:?}"
        )))
    })?;
    let height: u32 = h.trim().parse().map_err(|_| {
        CommandError::from(ForgeError::InvalidConfiguration(format!(
            "bad height in {raw:?}"
        )))
    })?;
    forge_core::ImageDimensions::new(width, height).map_err(CommandError::from)
}

fn parse_resize_args(
    width: Option<u32>,
    height: Option<u32>,
    fit: Option<String>,
    fill: Option<String>,
    filter: Option<String>,
    upscale: bool,
) -> Result<Option<forge_core::ResizeSpec>, CommandError> {
    use forge_core::ResizeSpec;
    let filter = parse_filter(filter.as_deref())?;
    if width.is_some() || height.is_some() {
        let (Some(w), Some(h)) = (width, height) else {
            return Err(ForgeError::InvalidConfiguration(
                "width and height must be given together".to_string(),
            )
            .into());
        };
        return Ok(Some(ResizeSpec::Exact {
            dimensions: forge_core::ImageDimensions::new(w, h).map_err(CommandError::from)?,
            filter,
        }));
    }
    if let Some(raw) = fill {
        return Ok(Some(ResizeSpec::Fill {
            bounds: parse_box(&raw)?,
            filter,
            upscale,
        }));
    }
    if let Some(raw) = fit {
        return Ok(Some(ResizeSpec::Fit {
            bounds: parse_box(&raw)?,
            filter,
            upscale,
        }));
    }
    Ok(None)
}

fn parse_compression_args(
    target: ImageFormat,
    quality: u8,
    png_level: Option<u8>,
    webp_lossless: bool,
) -> Result<Option<forge_core::Compression>, CommandError> {
    use forge_core::Compression;
    if target == ImageFormat::Png {
        if let Some(level) = png_level {
            if level > 9 {
                return Err(ForgeError::InvalidConfiguration(format!(
                    "PNG level must be 0-9, got {level}"
                ))
                .into());
            }
            return Ok(Some(Compression::Png { level }));
        }
        return Ok(None);
    }
    if target == ImageFormat::Webp && webp_lossless {
        return Ok(Some(Compression::WebpLossless));
    }
    if target == ImageFormat::Jpeg {
        return Ok(Some(Compression::Jpeg { quality }));
    }
    if target == ImageFormat::Webp {
        return Ok(Some(Compression::WebpLossy { quality }));
    }
    Ok(None)
}

fn engine() -> (
    StdFileSystem,
    ForgeImageDecoder,
    ForgeImageEncoder,
    ResizeStep,
) {
    (
        StdFileSystem,
        ForgeImageDecoder,
        ForgeImageEncoder,
        ResizeStep,
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
    let resize = parse_resize_args(
        args.width,
        args.height,
        args.fit.clone(),
        args.fill.clone(),
        args.filter.clone(),
        args.upscale.unwrap_or(false),
    )?;
    let compression = parse_compression_args(
        target,
        args.quality.unwrap_or(80),
        args.png_level,
        args.webp_lossless.unwrap_or(false),
    )?;
    let options = ConversionOptions {
        quality: args.quality.unwrap_or(80),
        max_dimensions,
        resize,
        compression,
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
    // Register a cancel flag so `cancel_convert` can abort mid-run.
    // Single-file converts finish fast, so cancellation usually lands
    // between files of a multi-file request (orchestrator checks per file).
    let job_id = forge_core::JobId::generate();
    let cancel = CANCELS.register(&job_id);
    *CURRENT_JOB.lock().expect("current job lock") = Some(job_id.clone());
    let result = orchestrator
        .run(request, &NullSink, &Cancellable(cancel))
        .map_err(|error| {
            CANCELS.remove(&job_id);
            *CURRENT_JOB.lock().expect("current job lock") = None;
            CommandError::from(error)
        })?;
    CANCELS.remove(&job_id);
    *CURRENT_JOB.lock().expect("current job lock") = None;
    let input_bytes: std::collections::HashMap<String, u64> = args
        .inputs
        .iter()
        .map(|input| {
            let bytes = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
            let stem = PathBuf::from(input)
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            (stem, bytes)
        })
        .collect();
    let outputs: Vec<String> = result
        .outputs
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    let sizes: Vec<FileSizes> = result
        .outputs
        .iter()
        .map(|p| {
            let output_bytes = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            let stem = p.file_stem().and_then(|n| n.to_str()).unwrap_or_default();
            FileSizes {
                output: p.display().to_string(),
                input_bytes: input_bytes.get(stem).copied().unwrap_or(0),
                output_bytes,
            }
        })
        .collect();
    Ok(ConvertDone {
        outputs,
        skipped: result
            .skipped
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        failures: result.failures.clone(),
        sizes,
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

/// `cancel_convert` — abort the in-flight conversion, if any.
/// Returns `"cancelled"` when a live job was signalled, `"idle"` otherwise.
/// The running `convert_image` settles with `Cancelled`; the UI treats that
/// like a clean stop, not a failure.
#[tauri::command]
pub fn cancel_convert() -> String {
    let current = CURRENT_JOB.lock().expect("current job lock").clone();
    match current {
        Some(id) if CANCELS.cancel(&id) => "cancelled".to_string(),
        _ => "idle".to_string(),
    }
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
