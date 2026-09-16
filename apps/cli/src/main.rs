//! forgeconvert CLI — thin presentation adapter over `forge-engine`.
//!
//! Same engine the desktop UI calls (ADR 010). Script-friendly:
//! non-zero exit per error variant, progress on stderr, machine-readable
//! `info` output. Commands mirror spec §30.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Parser, Subcommand, ValueEnum};
use forge_core::{
    ConversionOptions, ConversionRequest, ConversionResult, FileSystem as _, ForgeError,
    HistoryStore as _, ImageDecoder as _, ImageFormat, JobEventSink, JobId, JobProgress,
    MetadataPolicy, NeverCancel, OutputTarget, PdfRenderer as _, PdfWriteSpec, PdfWriter as _,
};
use forge_engine::{BatchConfig, CancelFlag, EngineDeps, NullSink, Orchestrator, StdFileSystem};
use forge_image::{FitWithinStep, ForgeImageDecoder, ForgeImageEncoder};
use forge_pdf::{ForgePdfWriter, StubPdfRenderer};

#[derive(Debug, Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Convert one image (png/jpg/webp/bmp/tiff → image).
    Convert {
        /// Input image file.
        input: PathBuf,
        /// Target format.
        #[arg(long, value_enum)]
        to: CliFormat,
        /// Output file or directory (default: beside input).
        #[arg(long)]
        output: Option<PathBuf>,
        /// Quality 1–100 (lossy targets).
        #[arg(long, default_value_t = 80)]
        quality: u8,
        /// Max width (aspect preserved).
        #[arg(long)]
        max_width: Option<u32>,
        /// Max height (aspect preserved).
        #[arg(long)]
        max_height: Option<u32>,
        /// Strip metadata.
        #[arg(long, default_value_t = false)]
        strip_metadata: bool,
        /// Existing-output behavior.
        #[arg(long, value_enum, default_value_t = CliCollision::Rename)]
        on_collision: CliCollision,
    },
    /// Convert a directory of images with a bounded worker pool.
    Batch {
        /// Input directory.
        dir: PathBuf,
        /// Target format.
        #[arg(long, value_enum)]
        to: CliFormat,
        /// Output directory (default: `<dir>/converted`).
        #[arg(long)]
        output_dir: Option<PathBuf>,
        /// Quality 1–100.
        #[arg(long, default_value_t = 80)]
        quality: u8,
        /// Max files in flight (default: CPU count).
        #[arg(long)]
        jobs: Option<usize>,
    },
    /// Pack images (one page each) into a PDF.
    Pdf {
        /// Input images.
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Output PDF file.
        #[arg(long)]
        output: PathBuf,
        /// Page size.
        #[arg(long, value_enum, default_value_t = CliPage::A4)]
        page: CliPage,
        /// Landscape orientation.
        #[arg(long, default_value_t = false)]
        landscape: bool,
    },
    /// Render PDF pages to images (stubbed until a renderer qualifies).
    Render {
        /// Input PDF.
        input: PathBuf,
        /// Output image format.
        #[arg(long, value_enum)]
        format: CliFormat,
        /// DPI 1–1200.
        #[arg(long, default_value_t = 200)]
        dpi: u16,
        /// Page range, e.g. `1-3` or `1,3,5`.
        #[arg(long)]
        pages: Option<String>,
    },
    /// Inspect an image (format, dimensions, alpha, size).
    Info {
        /// Input file.
        input: PathBuf,
    },
    /// Optimize for the web (WebP q80, metadata stripped) with savings report.
    Optimize {
        /// Input image.
        input: PathBuf,
        /// Output file or directory.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Preset key (web, hq-jpeg, small-jpeg, lossless-png, webp).
        #[arg(long, default_value_t = String::from("web"))]
        preset: String,
    },
    /// Show recent conversion history (local SQLite, metadata only).
    History {
        /// Max rows (default 20).
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

/// CLI image formats (PDF only where it makes sense per command).
#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliFormat {
    Png,
    Jpg,
    Webp,
    Bmp,
    Tiff,
}

impl CliFormat {
    fn to_image_format(self) -> ImageFormat {
        match self {
            Self::Png => ImageFormat::Png,
            Self::Jpg => ImageFormat::Jpeg,
            Self::Webp => ImageFormat::Webp,
            Self::Bmp => ImageFormat::Bmp,
            Self::Tiff => ImageFormat::Tiff,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliCollision {
    Rename,
    Replace,
    Skip,
    Fail,
}

impl CliCollision {
    fn to_policy(self) -> forge_core::CollisionPolicy {
        use forge_core::CollisionPolicy;
        match self {
            Self::Rename => CollisionPolicy::RenameAuto,
            Self::Replace => CollisionPolicy::Replace,
            Self::Skip => CollisionPolicy::Skip,
            Self::Fail => CollisionPolicy::Fail,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliPage {
    A4,
    Letter,
}

/// Stderr progress sink (stdout stays clean for scripts).
struct StderrSink;

impl JobEventSink for StderrSink {
    fn progress(&self, _id: &JobId, progress: &JobProgress) {
        if let Some(message) = &progress.message {
            eprintln!("  [{:.0}%] {message}", progress.fraction * 100.0);
        }
    }
    fn finished(&self, _id: &JobId, _result: &ConversionResult) {}
    fn failed(&self, _id: &JobId, error: &ForgeError) {
        eprintln!("  failed: {error}");
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let code = match run(cli) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("forgeconvert: error: {error}");
            exit_code_for(&error)
        }
    };
    std::process::exit(code);
}

/// Stable exit codes per error family (scripts can match on them).
/// Joined per-file failures surface OutputExists as 5 when the message
/// contains "output already exists" (orchestrator wraps the variant).
fn exit_code_for(error: &ForgeError) -> i32 {
    match error {
        ForgeError::InvalidConfiguration(_) => 2,
        ForgeError::UnsupportedFormat(_) => 3,
        ForgeError::InvalidFile(message) if message.contains("output already exists") => 5,
        ForgeError::InvalidFile(_) => 4,
        ForgeError::OutputExists(_) => 5,
        ForgeError::PermissionDenied(_) => 6,
        ForgeError::DiskFull(_) => 7,
        ForgeError::Cancelled => 130,
        ForgeError::Unsupported { .. } => 8,
        _ => 1,
    }
}

fn run(cli: Cli) -> Result<(), ForgeError> {
    match cli.command {
        Command::Convert {
            input,
            to,
            output,
            quality,
            max_width,
            max_height,
            strip_metadata,
            on_collision,
        } => {
            let max_dimensions = match (max_width, max_height) {
                (None, None) => None,
                (w, h) => Some(forge_core::ImageDimensions::new(
                    w.unwrap_or(u32::MAX),
                    h.unwrap_or(u32::MAX),
                )?),
            };
            let options = ConversionOptions {
                quality,
                max_dimensions,
                metadata: if strip_metadata {
                    MetadataPolicy::Remove
                } else {
                    MetadataPolicy::Preserve
                },
                background: Default::default(),
                on_collision: on_collision.to_policy(),
            }
            .validated()?;
            let target = to.to_image_format();
            let output_target = match output {
                Some(path) if is_explicit_file(&path) => OutputTarget::File(path),
                Some(dir) => OutputTarget::Directory(dir),
                None => OutputTarget::Directory(
                    input
                        .parent()
                        .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
                ),
            };
            // Single-file convert: resolve the output beside the input
            // (or into the given dir) via the engine's naming rules.
            let request = ConversionRequest {
                inputs: vec![input.clone()],
                output_format: target,
                output: output_target,
                options,
            }
            .validated()?;
            let fs = StdFileSystem;
            let decoder = ForgeImageDecoder;
            let encoder = ForgeImageEncoder;
            let fit = FitWithinStep;
            let transforms: [&dyn forge_core::TransformStep; 1] = [&fit];
            let engine = Orchestrator::new(EngineDeps {
                decoder: &decoder,
                encoder: &encoder,
                transforms: &transforms,
                fs: &fs,
            });
            let result = engine.run(request, &StderrSink, &NeverCancel)?;
            for path in &result.outputs {
                println!("{}", path.display());
            }
            for skipped in &result.skipped {
                // Skip echoes the existing path (stdout) + note (stderr) so
                // scripts keep a path while humans see why nothing was written.
                println!("{}", skipped.display());
                eprintln!("skipped (exists): {}", skipped.display());
            }
            for failure in &result.failures {
                eprintln!("forgeconvert: {failure}");
            }
            if result.outputs.is_empty() && result.skipped.is_empty() && !result.failures.is_empty()
            {
                return Err(ForgeError::InvalidFile(result.failures.join("; ")));
            }
            if let Some(first) = result.outputs.first() {
                record_history(
                    "convert",
                    &input,
                    first,
                    target,
                    &ConversionOptions {
                        quality,
                        ..Default::default()
                    },
                    0,
                );
            }
            Ok(())
        }
        Command::Batch {
            dir,
            to,
            output_dir,
            quality,
            jobs,
        } => {
            let target = to.to_image_format();
            let out_dir = output_dir.unwrap_or_else(|| dir.join("converted"));
            let entries = std::fs::read_dir(&dir).map_err(|e| {
                ForgeError::InvalidFile(format!("cannot list {}: {e}", dir.display()))
            })?;
            let mut inputs: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|entry| entry.path()))
                .filter(|p| {
                    p.extension()
                        .and_then(|x| x.to_str())
                        .and_then(ImageFormat::from_extension)
                        .is_some_and(|f| f != ImageFormat::Pdf)
                })
                .collect();
            inputs.sort();
            if inputs.is_empty() {
                return Err(ForgeError::InvalidFile(format!(
                    "no convertible images in {}",
                    dir.display()
                )));
            }
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|e| ForgeError::InvalidConfiguration(format!("tokio runtime: {e}")))?;
            runtime.block_on(async move {
                let config = BatchConfig {
                    concurrency: jobs.unwrap_or_else(|| {
                        std::thread::available_parallelism()
                            .map(|n| n.get())
                            .unwrap_or(4)
                    }),
                };
                let sink: Arc<dyn JobEventSink> = Arc::new(StderrSink);
                let job_id = JobId::generate();
                let cancel = CancelFlag::new();
                let report = forge_engine::run_batch(
                    inputs,
                    move |input: PathBuf| convert_one_sync(input, target, out_dir.clone(), quality),
                    &config,
                    sink,
                    &job_id,
                    cancel,
                )
                .await?;
                for path in &report.outputs {
                    println!("{}", path.display());
                }
                eprintln!(
                    "batch: {} ok, {} failed, {} skipped",
                    report.outputs.len(),
                    report.failures.len(),
                    report.skipped.len()
                );
                for failure in &report.failures {
                    eprintln!("forgeconvert: {failure}");
                }
                Ok::<(), ForgeError>(())
            })
        }
        Command::Pdf {
            inputs,
            output,
            page,
            landscape,
        } => {
            use forge_core::Orientation;
            let decoder = ForgeImageDecoder;
            let mut images = Vec::with_capacity(inputs.len());
            for input in &inputs {
                let bytes = StdFileSystem.read(input)?;
                let format = forge_core::detect_input_format(input, Some(&bytes))?;
                images.push(decoder.decode(&bytes, Some(format))?);
            }
            let spec = PdfWriteSpec {
                page: match page {
                    CliPage::A4 => forge_core::PageSizeMm::A4,
                    CliPage::Letter => forge_core::PageSizeMm::LETTER,
                },
                orientation: if landscape {
                    Orientation::Landscape
                } else {
                    Orientation::Portrait
                },
                ..Default::default()
            };
            let writer = ForgePdfWriter;
            let bytes = writer.write_images(&images, &spec)?;
            StdFileSystem.write_atomic(&output, &bytes)?;
            println!("{}", output.display());
            Ok(())
        }
        Command::Render {
            input,
            format,
            dpi,
            pages,
        } => {
            // Honest stub: surface Unsupported with the qualified-backend hint.
            let bytes = StdFileSystem.read(&input)?;
            let _format = forge_core::detect_input_format(&input, Some(&bytes))?;
            let _pages = pages
                .as_deref()
                .map(forge_core::PageRange::parse)
                .transpose()?;
            let _ = (format, dpi);
            let renderer = StubPdfRenderer;
            let _ = renderer.render(&bytes, &[1], dpi)?;
            Ok(())
        }
        Command::Info { input } => {
            let bytes = StdFileSystem.read(&input)?;
            let format = forge_core::detect_input_format(&input, Some(&bytes))?;
            let descriptor = format.descriptor();
            let meta = std::fs::metadata(&input)
                .map(|m| m.len())
                .unwrap_or(bytes.len() as u64);
            if format == ImageFormat::Pdf {
                println!("format: PDF ({})", descriptor.mime_types.join(","));
                println!("size: {meta} bytes");
                println!("note: page inspection lands with the PDF renderer (ADR 009)");
                return Ok(());
            }
            let decoder = ForgeImageDecoder;
            let image = decoder.decode(&bytes, Some(format))?;
            println!(
                "format: {} ({})",
                descriptor.name,
                descriptor.mime_types.join(",")
            );
            println!(
                "dimensions: {}x{}",
                image.dimensions.width, image.dimensions.height
            );
            println!("pixel: {:?}", image.pixel_format);
            println!("alpha: {}", image.has_alpha);
            println!("size: {meta} bytes");
            Ok(())
        }
        Command::Optimize {
            input,
            output,
            preset,
        } => {
            let named = forge_core::Preset::by_key(&preset).ok_or_else(|| {
                let keys: Vec<String> = forge_core::Preset::builtins()
                    .iter()
                    .map(|preset| preset.key())
                    .collect();
                ForgeError::InvalidConfiguration(format!(
                    "unknown preset {preset:?} (choose: {})",
                    keys.join(", ")
                ))
            })?;
            let before = std::fs::metadata(&input).map(|m| m.len()).unwrap_or(0);
            let options = named.options.clone().validated()?;
            let dir = output.clone().map_or_else(
                || {
                    input
                        .parent()
                        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
                },
                PathBuf::from,
            );
            // `output` may be a file or dir; reuse convert semantics.
            let output_target = match output {
                Some(path) if is_explicit_file(&path) => OutputTarget::File(path),
                _ => OutputTarget::Directory(dir),
            };
            let request = ConversionRequest {
                inputs: vec![input.clone()],
                output_format: named.output_format,
                output: output_target,
                options,
            }
            .validated()?;
            let fs = StdFileSystem;
            let decoder = ForgeImageDecoder;
            let encoder = ForgeImageEncoder;
            let fit = FitWithinStep;
            let transforms: [&dyn forge_core::TransformStep; 1] = [&fit];
            let engine = Orchestrator::new(EngineDeps {
                decoder: &decoder,
                encoder: &encoder,
                transforms: &transforms,
                fs: &fs,
            });
            let result = engine.run(request, &StderrSink, &NeverCancel)?;
            let out_path = result
                .outputs
                .first()
                .ok_or_else(|| ForgeError::InvalidFile(result.failures.join("; ")))?;
            let after = std::fs::metadata(out_path).map(|m| m.len()).unwrap_or(0);
            let savings = if before > 0 && after < before {
                f64::from((before - after) as u32) / f64::from(before as u32) * 100.0
            } else {
                0.0
            };
            println!("{}", out_path.display());
            eprintln!(
                "preset {}: before: {before} bytes; after: {after} bytes; saved: {savings:.1}%",
                named.name
            );
            record_history(
                "optimize",
                &input,
                out_path,
                named.output_format,
                &named.options,
                out_path.metadata().map(|m| m.len()).unwrap_or(0),
            );
            Ok(())
        }
        Command::History { limit } => {
            let db = open_history_db()?;
            let entries = forge_store::HistoryDb::recent(&db, limit)?;
            if entries.is_empty() {
                println!("no history yet");
                return Ok(());
            }
            for entry in entries {
                println!(
                    "{} | {} | {} → {} | {:?} | {}ms",
                    entry.job_id.0,
                    entry.operation,
                    entry.input_name,
                    entry.output_name,
                    entry.status,
                    entry.duration_ms
                );
            }
            Ok(())
        }
    }
}

/// Default history DB path: OS data dir or `.forgeconvert/history.db` fallback.
fn history_db_path() -> PathBuf {
    if let Some(dir) = dirs_data_dir() {
        return dir.join("forgeconvert").join("history.db");
    }
    PathBuf::from(".forgeconvert").join("history.db")
}

/// Platform data dir without a new dependency (env-first, then home).
fn dirs_data_dir() -> Option<PathBuf> {
    for key in ["XDG_DATA_HOME", "LOCALAPPDATA", "APPDATA"] {
        if let Ok(dir) = std::env::var(key) {
            if !dir.is_empty() {
                return Some(PathBuf::from(dir));
            }
        }
    }
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".local").join("share"))
}

/// Open (creating) the history DB; failures degrade to in-memory so a
/// read-only home never breaks conversion.
fn open_history_db() -> Result<forge_store::HistoryDb, ForgeError> {
    let path = history_db_path();
    forge_store::HistoryDb::open(&path).or_else(|_| forge_store::HistoryDb::in_memory())
}

/// Best-effort history write: never fails the conversion it records.
fn record_history(
    operation: &str,
    input: &Path,
    output: &Path,
    format: forge_core::ImageFormat,
    options: &ConversionOptions,
    duration_ms: u64,
) {
    let Ok(db) = open_history_db() else {
        return;
    };
    let entry = forge_core::HistoryEntry {
        job_id: JobId::generate(),
        operation: operation.to_string(),
        input_name: input
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string(),
        output_name: output
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string(),
        output_format: format,
        status: forge_core::JobStatus::Completed,
        duration_ms,
        options_json: format!(
            "{{\"quality\":{},\"metadata\":\"{:?}\"}}",
            options.quality, options.metadata
        ),
    };
    use forge_core::HistoryStore as _;
    let _ = db.record(entry);
}
/// `--output foo.webp` (has image extension) → explicit file;
/// `--output ./dir` → directory. Nonexistent paths with an image
/// extension count as files so `convert in.png --output out.webp` works.
fn is_explicit_file(path: &Path) -> bool {
    if path.is_dir() {
        return false;
    }
    path.extension()
        .and_then(|x| x.to_str())
        .and_then(ImageFormat::from_extension)
        .is_some()
}

/// Synchronous single-file conversion for the batch worker threads.
fn convert_one_sync(
    input: PathBuf,
    target: ImageFormat,
    out_dir: PathBuf,
    quality: u8,
) -> Result<Option<PathBuf>, ForgeError> {
    let fs = StdFileSystem;
    let decoder = ForgeImageDecoder;
    let encoder = ForgeImageEncoder;
    let fit = FitWithinStep;
    let transforms: [&dyn forge_core::TransformStep; 1] = [&fit];
    let engine = Orchestrator::new(EngineDeps {
        decoder: &decoder,
        encoder: &encoder,
        transforms: &transforms,
        fs: &fs,
    });
    let options = ConversionOptions {
        quality,
        ..Default::default()
    }
    .validated()?;
    let request = ConversionRequest {
        inputs: vec![input],
        output_format: target,
        output: OutputTarget::Directory(out_dir),
        options,
    }
    .validated()?;
    // Skip (not fail) same-format and missing files — batch keeps going.
    let result = engine.run(request, &NullSink, &NeverCancel)?;
    if let Some(path) = result.outputs.first() {
        return Ok(Some(path.clone()));
    }
    if !result.skipped.is_empty() {
        return Ok(None); // collision Skip → batch `skipped` bucket
    }
    Err(ForgeError::InvalidFile(result.failures.join("; ")))
}

/// Keep the flag type referenced for future Ctrl-C wiring (Phase 10).
#[allow(dead_code)]
fn cancel_flag() -> Arc<CancelFlag> {
    CancelFlag::new()
}

/// Silence helper documenting the default no-cancel path.
#[allow(dead_code)]
fn never_cancel_token() -> NeverCancel {
    NeverCancel
}

/// Re-export for tests asserting exit-code mapping.
#[allow(dead_code)]
fn __exit_code_for(error: &ForgeError) -> i32 {
    exit_code_for(error)
}
