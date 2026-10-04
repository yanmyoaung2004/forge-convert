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
use forge_pdf::{ForgePdfWriter, LopdfSplitter, LopdfToDocx};
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
    /// PDF page count (None for images / unreadable PDFs).
    pages: Option<u32>,
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
        use forge_core::PdfSplitter as _;
        let descriptor = format.descriptor();
        let pages = LopdfSplitter.page_count(&bytes).ok();
        return Ok(FileInfo {
            name,
            format: descriptor.name.to_string(),
            mime_type: descriptor.mime_types.join(","),
            width: None,
            height: None,
            pixel: None,
            alpha: false,
            size_bytes,
            pages,
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
        pages: None,
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
    if target == ImageFormat::Docx {
        return Err(ForgeError::InvalidConfiguration(
            "use pdf_to_docx for Word output (PDF input only)".to_string(),
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

/// `split_pdf` — keep `pages` (same `1-3` / `1,3,5-7` grammar as CLI).
/// Default output: `{stem}-split.pdf` beside input.
#[derive(Debug, Deserialize)]
pub(crate) struct SplitPdfArgs {
    input: String,
    pages: String,
    output: Option<String>,
    on_collision: Option<String>,
}

#[tauri::command]
pub fn split_pdf(args: SplitPdfArgs) -> CommandResult<String> {
    use forge_core::PdfSplitter as _;
    let input = PathBuf::from(&args.input);
    let fs = StdFileSystem;
    let bytes = fs.read(&input).map_err(CommandError::from)?;
    let detected =
        forge_core::detect_input_format(&input, Some(&bytes)).map_err(CommandError::from)?;
    if detected != ImageFormat::Pdf {
        return Err(ForgeError::UnsupportedFormat(format!(
            "split_pdf needs a PDF input, got {}",
            detected.mime_type()
        ))
        .into());
    }
    let range = PageRange::parse(&args.pages).map_err(CommandError::from)?;
    let out_bytes = LopdfSplitter
        .split(&bytes, &range)
        .map_err(CommandError::from)?;
    let candidate = match args.output {
        Some(out) => PathBuf::from(out),
        None => {
            let stem = forge_core::canonical_stem_of(&input);
            let dir = input
                .parent()
                .map_or_else(|| PathBuf::from("."), std::path::Path::to_path_buf);
            dir.join(format!("{stem}-split.pdf"))
        }
    };
    let policy = parse_collision(args.on_collision.as_deref())?;
    let target = forge_engine::apply_collision(&candidate, policy).map_err(CommandError::from)?;
    let Some(target) = target else {
        return Err(ForgeError::OutputExists(candidate).into());
    };
    fs.write_atomic(&target, &out_bytes)
        .map_err(CommandError::from)?;
    Ok(target.display().to_string())
}

/// `pdf_to_docx` — export PDF text to Word (.docx, text-only).
/// Default output: `{stem}.docx` beside input; `pages` omitted = all pages.
#[derive(Debug, Deserialize)]
pub(crate) struct PdfToDocxArgs {
    input: String,
    pages: Option<String>,
    output: Option<String>,
    on_collision: Option<String>,
}

#[tauri::command]
pub fn pdf_to_docx(args: PdfToDocxArgs) -> CommandResult<String> {
    use forge_core::PdfToDocx as _;
    let input = PathBuf::from(&args.input);
    let fs = StdFileSystem;
    let bytes = fs.read(&input).map_err(CommandError::from)?;
    let detected =
        forge_core::detect_input_format(&input, Some(&bytes)).map_err(CommandError::from)?;
    if detected != ImageFormat::Pdf {
        return Err(ForgeError::UnsupportedFormat(format!(
            "pdf_to_docx needs a PDF input, got {}",
            detected.mime_type()
        ))
        .into());
    }
    let range = args
        .pages
        .as_deref()
        .map(PageRange::parse)
        .transpose()
        .map_err(CommandError::from)?;
    let out_bytes = LopdfToDocx
        .convert(&bytes, range.as_ref())
        .map_err(CommandError::from)?;
    let candidate = match args.output {
        Some(out) => PathBuf::from(out),
        None => {
            let stem = forge_core::canonical_stem_of(&input);
            let dir = input
                .parent()
                .map_or_else(|| PathBuf::from("."), std::path::Path::to_path_buf);
            dir.join(format!("{stem}.docx"))
        }
    };
    let policy = parse_collision(args.on_collision.as_deref())?;
    let target = forge_engine::apply_collision(&candidate, policy).map_err(CommandError::from)?;
    let Some(target) = target else {
        return Err(ForgeError::OutputExists(candidate).into());
    };
    fs.write_atomic(&target, &out_bytes)
        .map_err(CommandError::from)?;
    Ok(target.display().to_string())
}

/// `pdf_page_count` — total pages (drives the desktop page-count line).
#[tauri::command]
pub fn pdf_page_count(input: String) -> CommandResult<u32> {
    use forge_core::PdfSplitter as _;
    let path = PathBuf::from(&input);
    let fs = StdFileSystem;
    let bytes = fs.read(&path).map_err(CommandError::from)?;
    LopdfSplitter.page_count(&bytes).map_err(CommandError::from)
}

/// `merge_pdfs` — concatenate PDFs in order. Default: `{first-stem}-merged.pdf`.
#[derive(Debug, Deserialize)]
pub(crate) struct MergePdfsArgs {
    inputs: Vec<String>,
    output: Option<String>,
    on_collision: Option<String>,
}

#[tauri::command]
pub fn merge_pdfs(args: MergePdfsArgs) -> CommandResult<String> {
    use forge_core::PdfMerger as _;
    use forge_pdf::LopdfMerger;
    if args.inputs.is_empty() {
        return Err(
            ForgeError::InvalidConfiguration("at least one input is required".to_string()).into(),
        );
    }
    let fs = StdFileSystem;
    let mut bufs: Vec<Vec<u8>> = Vec::with_capacity(args.inputs.len());
    for input in &args.inputs {
        let path = PathBuf::from(input);
        let bytes = fs.read(&path).map_err(CommandError::from)?;
        let detected =
            forge_core::detect_input_format(&path, Some(&bytes)).map_err(CommandError::from)?;
        if detected != ImageFormat::Pdf {
            return Err(ForgeError::UnsupportedFormat(format!(
                "merge_pdfs needs PDF inputs, got {} for {input}",
                detected.mime_type()
            ))
            .into());
        }
        bufs.push(bytes);
    }
    let refs: Vec<&[u8]> = bufs.iter().map(Vec::as_slice).collect();
    let out_bytes = LopdfMerger.merge(&refs).map_err(CommandError::from)?;
    let first = PathBuf::from(&args.inputs[0]);
    let candidate = match args.output {
        Some(out) => PathBuf::from(out),
        None => {
            let stem = forge_core::canonical_stem_of(&first);
            let dir = first
                .parent()
                .map_or_else(|| PathBuf::from("."), std::path::Path::to_path_buf);
            dir.join(format!("{stem}-merged.pdf"))
        }
    };
    let policy = parse_collision(args.on_collision.as_deref())?;
    let target = forge_engine::apply_collision(&candidate, policy).map_err(CommandError::from)?;
    let Some(target) = target else {
        return Err(ForgeError::OutputExists(candidate).into());
    };
    fs.write_atomic(&target, &out_bytes)
        .map_err(CommandError::from)?;
    Ok(target.display().to_string())
}

/// `compress_pdf` — prune orphans (light) or + recompress streams (balanced).
#[derive(Debug, Deserialize)]
pub(crate) struct CompressPdfArgs {
    input: String,
    level: Option<String>,
    output: Option<String>,
    on_collision: Option<String>,
}

#[tauri::command]
pub fn compress_pdf(args: CompressPdfArgs) -> CommandResult<String> {
    use forge_core::{PdfCompressLevel, PdfCompressor as _};
    use forge_pdf::LopdfCompressor;
    let input = PathBuf::from(&args.input);
    let fs = StdFileSystem;
    let bytes = fs.read(&input).map_err(CommandError::from)?;
    let detected =
        forge_core::detect_input_format(&input, Some(&bytes)).map_err(CommandError::from)?;
    if detected != ImageFormat::Pdf {
        return Err(ForgeError::UnsupportedFormat(format!(
            "compress_pdf needs a PDF input, got {}",
            detected.mime_type()
        ))
        .into());
    }
    let level = match args
        .level
        .as_deref()
        .unwrap_or("balanced")
        .to_ascii_lowercase()
        .as_str()
    {
        "light" => PdfCompressLevel::Light,
        "balanced" => PdfCompressLevel::Balanced,
        other => {
            return Err(ForgeError::InvalidConfiguration(format!(
                "unknown compress level {other:?} (choose: light, balanced)"
            ))
            .into());
        }
    };
    let out_bytes = LopdfCompressor
        .compress(&bytes, level)
        .map_err(CommandError::from)?;
    let candidate = match args.output {
        Some(out) => PathBuf::from(out),
        None => {
            let stem = forge_core::canonical_stem_of(&input);
            let dir = input
                .parent()
                .map_or_else(|| PathBuf::from("."), std::path::Path::to_path_buf);
            dir.join(format!("{stem}-compressed.pdf"))
        }
    };
    let policy = parse_collision(args.on_collision.as_deref())?;
    let target = forge_engine::apply_collision(&candidate, policy).map_err(CommandError::from)?;
    let Some(target) = target else {
        return Err(ForgeError::OutputExists(candidate).into());
    };
    fs.write_atomic(&target, &out_bytes)
        .map_err(CommandError::from)?;
    Ok(target.display().to_string())
}

/// `render_pdf` — render PDF pages to images via hayro. Returns written paths.
/// `pages` omitted/blank = all pages; `format` png|jpg|webp|bmp|tiff (default png).
#[derive(Debug, Deserialize)]
pub(crate) struct RenderPdfArgs {
    input: String,
    pages: Option<String>,
    dpi: Option<u16>,
    format: Option<String>,
    on_collision: Option<String>,
}

#[tauri::command]
pub fn render_pdf(args: RenderPdfArgs) -> CommandResult<Vec<String>> {
    use forge_core::{ImageEncoder as _, PdfRenderer as _, PdfSplitter as _};
    use forge_pdf::{HayroRenderer, LopdfSplitter};
    let input = PathBuf::from(&args.input);
    let fs = StdFileSystem;
    let bytes = fs.read(&input).map_err(CommandError::from)?;
    let detected =
        forge_core::detect_input_format(&input, Some(&bytes)).map_err(CommandError::from)?;
    if detected != ImageFormat::Pdf {
        return Err(ForgeError::UnsupportedFormat(format!(
            "render_pdf needs a PDF input, got {}",
            detected.mime_type()
        ))
        .into());
    }
    let target = args
        .format
        .as_deref()
        .map(ImageFormat::from_extension)
        .unwrap_or(Some(ImageFormat::Png))
        .ok_or_else(|| {
            ForgeError::InvalidConfiguration(format!("unknown render format {:?}", args.format))
        })?;
    if !ForgeImageEncoder::OUTPUTS.contains(&target) {
        return Err(ForgeError::UnsupportedFormat(format!(
            "render cannot write {}",
            target.mime_type()
        ))
        .into());
    }
    let page_list: Vec<u32> = match args.pages.as_deref().map(str::trim) {
        None | Some("") => {
            let total = LopdfSplitter
                .page_count(&bytes)
                .map_err(CommandError::from)?;
            (1..=total).collect()
        }
        Some(raw) => PageRange::parse(raw).map_err(CommandError::from)?.pages,
    };
    let dpi = args.dpi.unwrap_or(200);
    let renderer = HayroRenderer;
    let images = renderer
        .render(&bytes, &page_list, dpi)
        .map_err(CommandError::from)?;
    let stem = forge_core::canonical_stem_of(&input);
    let dir = input
        .parent()
        .map_or_else(|| PathBuf::from("."), std::path::Path::to_path_buf);
    let policy = parse_collision(args.on_collision.as_deref())?;
    let encoder = ForgeImageEncoder;
    let mut outputs = Vec::with_capacity(images.len());
    for (image, page) in images.iter().zip(page_list.iter()) {
        let out = encoder
            .encode(
                image,
                target,
                &ConversionOptions {
                    quality: 80,
                    ..Default::default()
                },
            )
            .map_err(CommandError::from)?;
        let candidate = dir.join(format!("{stem}-p{page}.{}", target.extension()));
        if let Some(t) =
            forge_engine::apply_collision(&candidate, policy).map_err(CommandError::from)?
        {
            fs.write_atomic(&t, &out).map_err(CommandError::from)?;
            outputs.push(t.display().to_string());
        }
    }
    Ok(outputs)
}

/// `favicon` — icon set from one image: favicon.ico (16/32/48) + sized PNGs.
/// Returns written paths (ico first), so the UI reveals the folder + snippet.
#[derive(Debug, Deserialize)]
pub(crate) struct FaviconArgs {
    input: String,
    output_dir: Option<String>,
    sizes: Option<String>,
    on_collision: Option<String>,
}
#[derive(Debug, Serialize)]
pub(crate) struct FaviconDone {
    outputs: Vec<String>,
    snippet: String,
}

#[tauri::command]
pub fn favicon(args: FaviconArgs) -> CommandResult<FaviconDone> {
    use forge_image::{ForgeImageDecoder, ForgeImageEncoder};
    let input = PathBuf::from(&args.input);
    let fs = StdFileSystem;
    let bytes = fs.read(&input).map_err(CommandError::from)?;
    let format =
        forge_core::detect_input_format(&input, Some(&bytes)).map_err(CommandError::from)?;
    let image = ForgeImageDecoder
        .decode(&bytes, Some(format))
        .map_err(CommandError::from)?;
    let wanted: Vec<u32> = args
        .sizes
        .as_deref()
        .unwrap_or("16,32,48,180,192,512")
        .split(',')
        .map(|s| {
            s.trim()
                .parse::<u32>()
                .map_err(|_| ForgeError::InvalidConfiguration(format!("bad favicon size: {s:?}")))
        })
        .collect::<Result<Vec<u32>, ForgeError>>()
        .map_err(CommandError::from)?;
    let icons = ForgeImageEncoder::generate_icons(&image, &wanted).map_err(CommandError::from)?;
    let out_dir = match args.output_dir {
        Some(dir) => PathBuf::from(dir),
        None => input
            .parent()
            .map_or_else(|| PathBuf::from("."), std::path::Path::to_path_buf)
            .join("icons"),
    };
    let policy = parse_collision(args.on_collision.as_deref())?;
    let ico_small: Vec<(u32, Vec<u8>)> = icons.iter().filter(|(s, _)| *s <= 256).cloned().collect();
    let ico_bytes = ForgeImageEncoder::pack_ico(&ico_small).map_err(CommandError::from)?;
    let mut outputs: Vec<String> = Vec::new();
    let mut snippet = String::from("<link rel=\"icon\" href=\"/favicon.ico\" sizes=\"any\">");
    let ico_candidate = out_dir.join("favicon.ico");
    if let Some(t) =
        forge_engine::apply_collision(&ico_candidate, policy).map_err(CommandError::from)?
    {
        fs.write_atomic(&t, &ico_bytes)
            .map_err(CommandError::from)?;
        outputs.push(t.display().to_string());
    }
    for (size, png) in &icons {
        let name = if *size == 180 {
            "apple-touch-icon.png".to_string()
        } else {
            format!("icon-{size}.png")
        };
        let candidate = out_dir.join(&name);
        if let Some(t) =
            forge_engine::apply_collision(&candidate, policy).map_err(CommandError::from)?
        {
            fs.write_atomic(&t, png).map_err(CommandError::from)?;
            outputs.push(t.display().to_string());
            if *size == 180 {
                snippet
                    .push_str("\n<link rel=\"apple-touch-icon\" href=\"/apple-touch-icon.png\">");
            } else if *size >= 192 {
                snippet.push_str(&format!(
                    "\n<link rel=\"icon\" type=\"image/png\" sizes=\"{size}x{size}\" href=\"/{name}\">"
                ));
            }
        }
    }
    Ok(FaviconDone { outputs, snippet })
}

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
