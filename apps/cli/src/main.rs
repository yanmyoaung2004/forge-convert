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
use forge_image::{ForgeImageDecoder, ForgeImageEncoder, QrOutput, ResizeStep};
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
        /// Exact width (needs --height; forces size, aspect may change).
        #[arg(long, requires = "height")]
        width: Option<u32>,
        /// Exact height (needs --width; forces size, aspect may change).
        #[arg(long, requires = "width")]
        height: Option<u32>,
        /// Fit inside WIDTHxHEIGHT (e.g. 800x600), aspect kept.
        #[arg(long, value_name = "WIDTHxHEIGHT", conflicts_with_all = ["width", "max_width"])]
        fit: Option<String>,
        /// Cover WIDTHxHEIGHT then center-crop, aspect kept.
        #[arg(long, value_name = "WIDTHxHEIGHT", conflicts_with_all = ["width", "max_width"])]
        fill: Option<String>,
        /// Max width, legacy fit-inside (aspect preserved).
        #[arg(long, conflicts_with = "fit")]
        max_width: Option<u32>,
        /// Max height, legacy fit-inside (aspect preserved).
        #[arg(long, conflicts_with = "fit")]
        max_height: Option<u32>,
        /// Resample filter.
        #[arg(long, value_enum, default_value_t = CliFilter::Lanczos3)]
        filter: CliFilter,
        /// Allow upscaling small images to fit/fill boxes.
        #[arg(long, default_value_t = false)]
        upscale: bool,
        /// PNG compression level 0 (fast) – 9 (smallest).
        #[arg(long, value_name = "0-9")]
        png_level: Option<u8>,
        /// WebP lossless (exact pixels) instead of lossy quality.
        #[arg(long, default_value_t = false, conflicts_with = "quality")]
        webp_lossless: bool,
        /// Strip metadata.
        #[arg(long, default_value_t = false)]
        strip_metadata: bool,
        /// Existing-output behavior.
        #[arg(long, value_enum, default_value_t = CliCollision::Rename)]
        on_collision: CliCollision,
    },
    /// Convert a directory of images with a bounded worker pool.
    /// Same resize/compression flags as `convert` (parity by construction:
    /// both arms build options through `shared_options`).
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
        /// Exact width (needs --height; forces size, aspect may change).
        #[arg(long, requires = "height")]
        width: Option<u32>,
        /// Exact height (needs --width; forces size, aspect may change).
        #[arg(long, requires = "width")]
        height: Option<u32>,
        /// Fit inside WIDTHxHEIGHT (e.g. 800x600), aspect kept.
        #[arg(long, value_name = "WIDTHxHEIGHT", conflicts_with_all = ["width", "max_width"])]
        fit: Option<String>,
        /// Cover WIDTHxHEIGHT then center-crop, aspect kept.
        #[arg(long, value_name = "WIDTHxHEIGHT", conflicts_with_all = ["width", "max_width"])]
        fill: Option<String>,
        /// Max width, legacy fit-inside (aspect preserved).
        #[arg(long, conflicts_with = "fit")]
        max_width: Option<u32>,
        /// Max height, legacy fit-inside (aspect preserved).
        #[arg(long, conflicts_with = "fit")]
        max_height: Option<u32>,
        /// Resample filter.
        #[arg(long, value_enum, default_value_t = CliFilter::Lanczos3)]
        filter: CliFilter,
        /// Allow upscaling small images to fit/fill boxes.
        #[arg(long, default_value_t = false)]
        upscale: bool,
        /// PNG compression level 0 (fast) – 9 (smallest).
        #[arg(long, value_name = "0-9")]
        png_level: Option<u8>,
        /// WebP lossless (exact pixels) instead of lossy quality.
        #[arg(long, default_value_t = false, conflicts_with = "quality")]
        webp_lossless: bool,
        /// Strip metadata.
        #[arg(long, default_value_t = false)]
        strip_metadata: bool,
        /// Existing-output behavior.
        #[arg(long, value_enum, default_value_t = CliCollision::Rename)]
        on_collision: CliCollision,
        /// Max files in flight (default: CPU count).
        #[arg(long)]
        jobs: Option<usize>,
    },
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
    /// Split PDF pages (e.g. `--pages 2-5` keeps pages 2–5).
    PdfSplit {
        /// Input PDF.
        input: PathBuf,
        /// Page range, e.g. `1-3` or `1,3,5-7` (same grammar as `render --pages`).
        #[arg(long)]
        pages: String,
        /// Output PDF file (default: `{stem}-split.pdf` beside input).
        #[arg(long)]
        output: Option<PathBuf>,
        /// Existing-output behavior.
        #[arg(long, value_enum, default_value_t = CliCollision::Rename)]
        on_collision: CliCollision,
    },
    /// Export PDF text to Word (.docx, text-only, one paragraph per line).
    PdfToDocx {
        /// Input PDF.
        input: PathBuf,
        /// Page range, e.g. `1-3` (default: all pages).
        #[arg(long)]
        pages: Option<String>,
        /// Output .docx file (default: `{stem}.docx` beside input).
        #[arg(long)]
        output: Option<PathBuf>,
        /// Existing-output behavior.
        #[arg(long, value_enum, default_value_t = CliCollision::Rename)]
        on_collision: CliCollision,
    },
    /// Merge several PDFs in order (pages concatenated).
    PdfMerge {
        /// Input PDFs (at least one).
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Output PDF file (default: `{first-stem}-merged.pdf` beside first input).
        #[arg(long)]
        output: Option<PathBuf>,
        /// Existing-output behavior.
        #[arg(long, value_enum, default_value_t = CliCollision::Rename)]
        on_collision: CliCollision,
    },
    /// Compress a PDF (prune orphans; balanced also recompresses streams).
    PdfCompress {
        /// Input PDF.
        input: PathBuf,
        /// Compression effort.
        #[arg(long, value_enum, default_value_t = CliCompress::Balanced)]
        level: CliCompress,
        /// Output PDF file (default: `{stem}-compressed.pdf` beside input).
        #[arg(long)]
        output: Option<PathBuf>,
        /// Existing-output behavior.
        #[arg(long, value_enum, default_value_t = CliCollision::Rename)]
        on_collision: CliCollision,
    },
    /// Generate favicon set: favicon.ico (16/32/48) + sized PNGs + link snippet.
    Favicon {
        /// Input image (square works best; auto-resized with Lanczos3).
        input: PathBuf,
        /// Output directory (default: `<input-dir>/icons`).
        #[arg(long)]
        output_dir: Option<PathBuf>,
        /// Comma-separated sizes 16–512 (default: `16,32,48,180,192,512`).
        #[arg(long, default_value_t = String::from("16,32,48,180,192,512"))]
        sizes: String,
        #[arg(long, value_enum, default_value_t = CliCollision::Rename)]
        on_collision: CliCollision,
    },
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
    /// Render text as QR (PNG or SVG, pure-Rust `qrcode` + `image`).
    Qr {
        /// Text to encode (max 2048 bytes).
        text: String,
        /// Output file (default: `qr.png` in cwd; `.svg` when `--format svg` and no output given).
        #[arg(long)]
        output: Option<PathBuf>,
        /// Longest side in px, 128–1024 (default 256; PNG only).
        #[arg(long, default_value_t = 256)]
        size: u32,
        /// Error correction: L (7%) | M (15%) | Q (25%) | H (30%). Default M.
        #[arg(long, default_value_t = String::from("M"))]
        ec: String,
        /// Output format: png | svg. Default png.
        #[arg(long, default_value_t = String::from("png"))]
        format: String,
        /// Drop the white quiet-zone border (default: keep it).
        #[arg(long, default_value_t = false)]
        no_quiet_zone: bool,
        /// Existing-output behavior.
        #[arg(long, value_enum, default_value_t = CliCollision::Rename)]
        on_collision: CliCollision,
    },
    /// Render one QR per line of a text file (one payload per line, `#` comments + blanks skipped).
    QrBatch {
        /// List file (UTF-8; CRLF + BOM tolerated).
        list: PathBuf,
        /// Output directory (default: `qr-batch` in cwd).
        out_dir: Option<PathBuf>,
        #[arg(long, default_value_t = 256)]
        size: u32,
        /// Error correction: L (7%) | M (15%) | Q (25%) | H (30%). Default M.
        #[arg(long, default_value_t = String::from("M"))]
        ec: String,
        /// Output format: png | svg. Default png.
        #[arg(long, default_value_t = String::from("png"))]
        format: String,
        /// Drop the white quiet-zone border (default: keep it).
        #[arg(long, default_value_t = false)]
        no_quiet_zone: bool,
        /// Existing-output behavior.
        #[arg(long, value_enum, default_value_t = CliCollision::Rename)]
        on_collision: CliCollision,
    },
    /// SHA-256 hex of a file (streamed; 512 MiB cap).
    Hash {
        /// Input file.
        input: PathBuf,
    },
    /// Decode a QR code from an image file (prints text to stdout).
    QrDecode {
        /// Input image (PNG/JPEG/WebP/BMP/TIFF).
        input: PathBuf,
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
enum CliPage {
    A4,
    Letter,
}

/// Resample filter (maps 1:1 to domain `ResizeFilter`).
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum CliFilter {
    #[default]
    Lanczos3,
    CatmullRom,
    Gaussian,
    Nearest,
}

impl CliFilter {
    fn to_domain(self) -> forge_core::ResizeFilter {
        match self {
            Self::Lanczos3 => forge_core::ResizeFilter::Lanczos3,
            Self::CatmullRom => forge_core::ResizeFilter::CatmullRom,
            Self::Gaussian => forge_core::ResizeFilter::Gaussian,
            Self::Nearest => forge_core::ResizeFilter::Nearest,
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

/// PDF compression effort (maps 1:1 to domain `PdfCompressLevel`).
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum CliCompress {
    Light,
    #[default]
    Balanced,
}

impl CliCompress {
    fn to_domain(self) -> forge_core::PdfCompressLevel {
        match self {
            Self::Light => forge_core::PdfCompressLevel::Light,
            Self::Balanced => forge_core::PdfCompressLevel::Balanced,
        }
    }
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
            width,
            height,
            fit,
            fill,
            max_width,
            max_height,
            filter,
            upscale,
            png_level,
            webp_lossless,
            strip_metadata,
            on_collision,
        } => {
            let target = to.to_image_format();
            let options = shared_options(
                target,
                quality,
                width,
                height,
                fit,
                fill,
                max_width,
                max_height,
                filter,
                upscale,
                png_level,
                webp_lossless,
                strip_metadata,
                on_collision,
            )?;
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
            let fit = ResizeStep;
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
            width,
            height,
            fit,
            fill,
            max_width,
            max_height,
            filter,
            upscale,
            png_level,
            webp_lossless,
            strip_metadata,
            on_collision,
            jobs,
        } => {
            let target = to.to_image_format();
            // Same builder as `convert` — parity by construction.
            let template = shared_options(
                target,
                quality,
                width,
                height,
                fit,
                fill,
                max_width,
                max_height,
                filter,
                upscale,
                png_level,
                webp_lossless,
                strip_metadata,
                on_collision,
            )?;
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
                        .is_some_and(|f| f != ImageFormat::Pdf && f != ImageFormat::Docx)
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
                    move |input: PathBuf| {
                        convert_one_sync(input, target, out_dir.clone(), template.clone())
                    },
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
        Command::PdfSplit {
            input,
            pages,
            output,
            on_collision,
        } => {
            use forge_core::{PageRange, PdfSplitter as _};
            use forge_pdf::LopdfSplitter;
            let bytes = StdFileSystem.read(&input)?;
            let detected = forge_core::detect_input_format(&input, Some(&bytes))?;
            if detected != ImageFormat::Pdf {
                return Err(ForgeError::UnsupportedFormat(format!(
                    "pdf-split needs a PDF input, got {}",
                    detected.mime_type()
                )));
            }
            let range = PageRange::parse(&pages)?;
            let out_bytes = LopdfSplitter.split(&bytes, &range)?;
            let candidate = output.unwrap_or_else(|| {
                let stem = forge_core::canonical_stem_of(&input);
                let dir = input
                    .parent()
                    .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
                dir.join(format!("{stem}-split.pdf"))
            });
            let target = resolve_explicit_output(&candidate, on_collision.to_policy())?;
            let Some(target) = target else {
                println!("{}", candidate.display());
                eprintln!("skipped (exists): {}", candidate.display());
                return Ok(());
            };
            StdFileSystem.write_atomic(&target, &out_bytes)?;
            println!("{}", target.display());
            record_history(
                "pdf-split",
                &input,
                &target,
                ImageFormat::Pdf,
                &ConversionOptions::default(),
                0,
            );
            Ok(())
        }
        Command::PdfToDocx {
            input,
            pages,
            output,
            on_collision,
        } => {
            use forge_core::{PageRange, PdfToDocx as _};
            use forge_pdf::LopdfToDocx;
            let bytes = StdFileSystem.read(&input)?;
            let detected = forge_core::detect_input_format(&input, Some(&bytes))?;
            if detected != ImageFormat::Pdf {
                return Err(ForgeError::UnsupportedFormat(format!(
                    "pdf-to-docx needs a PDF input, got {}",
                    detected.mime_type()
                )));
            }
            let range = pages.as_deref().map(PageRange::parse).transpose()?;
            let out_bytes = LopdfToDocx.convert(&bytes, range.as_ref())?;
            let candidate = output.unwrap_or_else(|| {
                let stem = forge_core::canonical_stem_of(&input);
                let dir = input
                    .parent()
                    .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
                dir.join(format!("{stem}.docx"))
            });
            let target = resolve_explicit_output(&candidate, on_collision.to_policy())?;
            let Some(target) = target else {
                println!("{}", candidate.display());
                eprintln!("skipped (exists): {}", candidate.display());
                return Ok(());
            };
            StdFileSystem.write_atomic(&target, &out_bytes)?;
            println!("{}", target.display());
            record_history(
                "pdf-to-docx",
                &input,
                &target,
                ImageFormat::Docx,
                &ConversionOptions::default(),
                0,
            );
            Ok(())
        }
        Command::PdfMerge {
            inputs,
            output,
            on_collision,
        } => {
            use forge_core::PdfMerger as _;
            use forge_pdf::LopdfMerger;
            let mut bufs: Vec<Vec<u8>> = Vec::with_capacity(inputs.len());
            for input in &inputs {
                let bytes = StdFileSystem.read(input)?;
                let detected = forge_core::detect_input_format(input, Some(&bytes))?;
                if detected != ImageFormat::Pdf {
                    return Err(ForgeError::UnsupportedFormat(format!(
                        "pdf-merge needs PDF inputs, got {} for {}",
                        detected.mime_type(),
                        input.display()
                    )));
                }
                bufs.push(bytes);
            }
            let refs: Vec<&[u8]> = bufs.iter().map(Vec::as_slice).collect();
            let out_bytes = LopdfMerger.merge(&refs)?;
            let first = &inputs[0];
            let candidate = output.unwrap_or_else(|| {
                let stem = forge_core::canonical_stem_of(first);
                let dir = first
                    .parent()
                    .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
                dir.join(format!("{stem}-merged.pdf"))
            });
            let target = resolve_explicit_output(&candidate, on_collision.to_policy())?;
            let Some(target) = target else {
                println!("{}", candidate.display());
                eprintln!("skipped (exists): {}", candidate.display());
                return Ok(());
            };
            let before: u64 = bufs.iter().map(|b| b.len() as u64).sum();
            StdFileSystem.write_atomic(&target, &out_bytes)?;
            println!("{}", target.display());
            eprintln!(
                "merged {} files: before: {before} bytes; after: {} bytes",
                inputs.len(),
                out_bytes.len()
            );
            record_history(
                "pdf-merge",
                first,
                &target,
                ImageFormat::Pdf,
                &ConversionOptions::default(),
                0,
            );
            Ok(())
        }
        Command::PdfCompress {
            input,
            level,
            output,
            on_collision,
        } => {
            use forge_core::PdfCompressor as _;
            use forge_pdf::LopdfCompressor;
            let bytes = StdFileSystem.read(&input)?;
            let detected = forge_core::detect_input_format(&input, Some(&bytes))?;
            if detected != ImageFormat::Pdf {
                return Err(ForgeError::UnsupportedFormat(format!(
                    "pdf-compress needs a PDF input, got {}",
                    detected.mime_type()
                )));
            }
            let out_bytes = LopdfCompressor.compress(&bytes, level.to_domain())?;
            let candidate = output.unwrap_or_else(|| {
                let stem = forge_core::canonical_stem_of(&input);
                let dir = input
                    .parent()
                    .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
                dir.join(format!("{stem}-compressed.pdf"))
            });
            let target = resolve_explicit_output(&candidate, on_collision.to_policy())?;
            let Some(target) = target else {
                println!("{}", candidate.display());
                eprintln!("skipped (exists): {}", candidate.display());
                return Ok(());
            };
            StdFileSystem.write_atomic(&target, &out_bytes)?;
            println!("{}", target.display());
            let saved = if !bytes.is_empty() && out_bytes.len() < bytes.len() {
                (bytes.len() - out_bytes.len()) as f64 / bytes.len() as f64 * 100.0
            } else {
                0.0
            };
            eprintln!(
                "compressed: before: {} bytes; after: {} bytes; saved: {saved:.1}%",
                bytes.len(),
                out_bytes.len()
            );
            record_history(
                "pdf-compress",
                &input,
                &target,
                ImageFormat::Pdf,
                &ConversionOptions::default(),
                0,
            );
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
            let fit = ResizeStep;
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
        Command::Favicon {
            input,
            output_dir,
            sizes,
            on_collision,
        } => {
            use forge_image::ForgeImageEncoder;
            let bytes = StdFileSystem.read(&input)?;
            let format = forge_core::detect_input_format(&input, Some(&bytes))?;
            let image = ForgeImageDecoder.decode(&bytes, Some(format))?;
            let wanted: Result<Vec<u32>, ForgeError> = sizes
                .split(',')
                .map(|s| {
                    s.trim().parse::<u32>().map_err(|_| {
                        ForgeError::InvalidConfiguration(format!("bad favicon size: {s:?}"))
                    })
                })
                .collect();
            let wanted = wanted?;
            let icons = ForgeImageEncoder::generate_icons(&image, &wanted)?;
            let out_dir = output_dir.unwrap_or_else(|| {
                input
                    .parent()
                    .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
                    .join("icons")
            });
            let policy = on_collision.to_policy();
            // favicon.ico from the ≤256 frames.
            let ico_small: Vec<(u32, Vec<u8>)> =
                icons.iter().filter(|(s, _)| *s <= 256).cloned().collect();
            let ico_bytes = ForgeImageEncoder::pack_ico(&ico_small)?;
            let ico_path = out_dir.join("favicon.ico");
            let ico_target = resolve_explicit_output(&ico_path, policy)?;
            let mut written: Vec<(u32, PathBuf)> = Vec::new();
            if let Some(t) = ico_target {
                StdFileSystem.write_atomic(&t, &ico_bytes)?;
                println!("{}", t.display());
            }
            for (size, png) in &icons {
                let name = if *size == 180 {
                    "apple-touch-icon.png".to_string()
                } else {
                    format!("icon-{size}.png")
                };
                let candidate = out_dir.join(name);
                match resolve_explicit_output(&candidate, policy)? {
                    Some(t) => {
                        StdFileSystem.write_atomic(&t, png)?;
                        println!("{}", t.display());
                        written.push((*size, t));
                    }
                    None => {
                        println!("{}", candidate.display());
                        eprintln!("skipped (exists): {}", candidate.display());
                    }
                }
            }
            // Copy-paste HTML snippet on stdout (scripts grab paths above).
            println!("<link rel=\"icon\" href=\"/favicon.ico\" sizes=\"any\">");
            for (size, path) in &written {
                let fname = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("icon.png");
                if *size == 180 {
                    println!("<link rel=\"apple-touch-icon\" href=\"/{fname}\">");
                } else if *size >= 192 {
                    println!("<link rel=\"icon\" type=\"image/png\" sizes=\"{size}x{size}\" href=\"/{fname}\">");
                }
            }
            record_history(
                "favicon",
                &input,
                &out_dir.join("favicon.ico"),
                ImageFormat::Png,
                &ConversionOptions::default(),
                0,
            );
            Ok(())
        }
        Command::Qr {
            text,
            output,
            size,
            ec,
            format,
            no_quiet_zone,
            on_collision,
        } => {
            let quiet = !no_quiet_zone;
            let out = ForgeImageEncoder::encode_qr(&text, Some(&ec), Some(&format), size, quiet)?;
            let (default_name, bytes) = match &out {
                QrOutput::Png(png) => ("qr.png", png.clone()),
                QrOutput::Svg(svg) => ("qr.svg", svg.as_bytes().to_vec()),
            };
            let candidate = output.unwrap_or_else(|| PathBuf::from(default_name));
            let target = resolve_explicit_output(&candidate, on_collision.to_policy())?;
            let Some(target) = target else {
                println!("{}", candidate.display());
                eprintln!("skipped (exists): {}", candidate.display());
                return Ok(());
            };
            StdFileSystem.write_atomic(&target, &bytes)?;
            println!("{}", target.display());
            record_history(
                "qr",
                &PathBuf::from(&text.chars().take(32).collect::<String>()),
                &target,
                ImageFormat::Png,
                &ConversionOptions::default(),
                0,
            );
            Ok(())
        }
        Command::Hash { input } => {
            use sha2::{Digest, Sha256};
            let bytes = StdFileSystem.read(&input)?;
            let mut hasher = Sha256::new();
            for chunk in bytes.chunks(64 * 1024) {
                hasher.update(chunk);
            }
            println!("{:x}", hasher.finalize());
            Ok(())
        }
        Command::QrDecode { input } => {
            let bytes = StdFileSystem.read(&input)?;
            let text = ForgeImageEncoder::decode_qr(&bytes)?;
            println!("{text}");
            record_history(
                "qr-decode",
                &input,
                &input,
                ImageFormat::Png,
                &ConversionOptions::default(),
                0,
            );
            Ok(())
        }
        Command::QrBatch {
            list,
            out_dir,
            size,
            ec,
            format,
            no_quiet_zone,
            on_collision,
        } => {
            // CRLF tolerated (trim), BOM stripped (Windows-authored lists).
            let raw = StdFileSystem.read(&list)?;
            let text = String::from_utf8(raw)
                .map_err(|e| ForgeError::InvalidFile(format!("qr-batch list not UTF-8: {e}")))?;
            let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
            let payloads: Vec<&str> = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect();
            if payloads.is_empty() {
                return Err(ForgeError::InvalidConfiguration(
                    "qr-batch list has no payloads".to_string(),
                ));
            }
            let dir = out_dir.unwrap_or_else(|| PathBuf::from("qr-batch"));
            let policy = on_collision.to_policy();
            let ext = match format.to_ascii_lowercase().as_str() {
                "svg" => "svg",
                "png" => "png",
                other => {
                    return Err(ForgeError::InvalidConfiguration(format!(
                        "qr format must be png|svg, got {other:?}"
                    )));
                }
            };
            let mut ok = 0usize;
            let mut failed = 0usize;
            let mut skipped = 0usize;
            for (i, payload) in payloads.iter().enumerate() {
                let quiet = !no_quiet_zone;
                match ForgeImageEncoder::encode_qr(payload, Some(&ec), Some(&format), size, quiet) {
                    Ok(out) => {
                        let bytes = match &out {
                            QrOutput::Png(png) => png.clone(),
                            QrOutput::Svg(svg) => svg.as_bytes().to_vec(),
                        };
                        let candidate = dir.join(format!("qr-{:03}.{ext}", i + 1));
                        match resolve_explicit_output(&candidate, policy)? {
                            Some(t) => {
                                StdFileSystem.write_atomic(&t, &bytes)?;
                                println!("{}", t.display());
                                ok += 1;
                            }
                            None => {
                                println!("{}", candidate.display());
                                eprintln!("skipped (exists): {}", candidate.display());
                                skipped += 1;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("line {}: {e}", i + 1);
                        failed += 1;
                    }
                }
            }
            eprintln!("{ok} ok / {failed} failed / {skipped} skipped");
            if ok == 0 && skipped == 0 && failed > 0 {
                return Err(ForgeError::InvalidFile(
                    "qr-batch: all payloads failed".to_string(),
                ));
            }
            if let Some(first) = std::fs::read_dir(&dir)
                .ok()
                .and_then(|mut d| d.next())
                .and_then(|e| e.ok())
            {
                record_history(
                    "qr-batch",
                    &list,
                    &first.path(),
                    ImageFormat::Png,
                    &ConversionOptions::default(),
                    0,
                );
            }
            Ok(())
        }
    }
}

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

/// Apply the collision policy to an EXPLICIT file candidate (pdf-split /
/// pdf-to-docx defaults are already concrete paths — no stem-derivation).
/// Returns `Ok(None)` for Skip-when-exists (caller echoes + returns).
fn resolve_explicit_output(
    candidate: &Path,
    policy: forge_core::CollisionPolicy,
) -> Result<Option<PathBuf>, ForgeError> {
    forge_engine::apply_collision(candidate, policy)
}

/// Parse `WIDTHxHEIGHT` (e.g. `800x600`, case-insensitive `x`).
fn parse_box(raw: &str) -> Result<forge_core::ImageDimensions, ForgeError> {
    let (w, h) = raw.split_once(['x', 'X']).ok_or_else(|| {
        ForgeError::InvalidConfiguration(format!("expected WIDTHxHEIGHT, got {raw:?}"))
    })?;
    let width: u32 = w
        .trim()
        .parse()
        .map_err(|_| ForgeError::InvalidConfiguration(format!("bad width in {raw:?}")))?;
    let height: u32 = h
        .trim()
        .parse()
        .map_err(|_| ForgeError::InvalidConfiguration(format!("bad height in {raw:?}")))?;
    forge_core::ImageDimensions::new(width, height)
}

/// Build the explicit [`ResizeSpec`] from convert flags.
/// Precedence: `--width/--height` exact > `--fill` > `--fit`.
/// Legacy `--max-*` flows through `max_dimensions` (never upscale).
#[allow(clippy::too_many_arguments)]
fn parse_resize(
    width: Option<u32>,
    height: Option<u32>,
    fit: Option<String>,
    fill: Option<String>,
    max_width: Option<u32>,
    max_height: Option<u32>,
    filter: CliFilter,
    upscale: bool,
) -> Result<Option<forge_core::ResizeSpec>, ForgeError> {
    use forge_core::ResizeSpec;
    // Exact wins when present. (`--height` requires `--width` and vice versa,
    // enforced by clap; legacy max-* without explicit resize stays `None`.)
    if width.is_some() || height.is_some() {
        let (Some(w), Some(h)) = (width, height) else {
            return Err(ForgeError::InvalidConfiguration(
                "--width and --height must be given together".to_string(),
            ));
        };
        return Ok(Some(ResizeSpec::Exact {
            dimensions: forge_core::ImageDimensions::new(w, h)?,
            filter: filter.to_domain(),
        }));
    }
    if let Some(raw) = fill {
        return Ok(Some(ResizeSpec::Fill {
            bounds: parse_box(&raw)?,
            filter: filter.to_domain(),
            upscale: upscale || max_width.is_some() || max_height.is_some(),
        }));
    }
    if let Some(raw) = fit {
        return Ok(Some(ResizeSpec::Fit {
            bounds: parse_box(&raw)?,
            filter: filter.to_domain(),
            upscale,
        }));
    }
    Ok(None)
}

/// Build the [`Compression`] override from convert flags.
/// - `--png-level N` → PNG level (PNG targets only; else ignored later).
/// - `--webp-lossless` → WebP lossless (WebP targets only).
/// - JPEG always uses `--quality` (explicit or default 80).
fn parse_compression(
    target: ImageFormat,
    quality: u8,
    png_level: Option<u8>,
    webp_lossless: bool,
) -> Result<Option<forge_core::Compression>, ForgeError> {
    use forge_core::Compression;
    if target == ImageFormat::Png {
        if let Some(level) = png_level {
            if level > 9 {
                return Err(ForgeError::InvalidConfiguration(format!(
                    "PNG level must be 0-9, got {level}"
                )));
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

/// One builder for `convert` and `batch` (parity by construction).
/// Both arms pass their identical flags here; divergence is a compile error.
#[allow(clippy::too_many_arguments)]
fn shared_options(
    target: ImageFormat,
    quality: u8,
    width: Option<u32>,
    height: Option<u32>,
    fit: Option<String>,
    fill: Option<String>,
    max_width: Option<u32>,
    max_height: Option<u32>,
    filter: CliFilter,
    upscale: bool,
    png_level: Option<u8>,
    webp_lossless: bool,
    strip_metadata: bool,
    on_collision: CliCollision,
) -> Result<ConversionOptions, ForgeError> {
    let resize = parse_resize(
        width, height, fit, fill, max_width, max_height, filter, upscale,
    )?;
    let compression = parse_compression(target, quality, png_level, webp_lossless)?;
    let max_dimensions = match (max_width, max_height) {
        (None, None) => None,
        (w, h) => Some(forge_core::ImageDimensions::new(
            w.unwrap_or(u32::MAX),
            h.unwrap_or(u32::MAX),
        )?),
    };
    ConversionOptions {
        quality,
        max_dimensions,
        resize,
        compression,
        metadata: if strip_metadata {
            MetadataPolicy::Remove
        } else {
            MetadataPolicy::Preserve
        },
        background: Default::default(),
        on_collision: on_collision.to_policy(),
    }
    .validated()
}
fn convert_one_sync(
    input: PathBuf,
    target: ImageFormat,
    out_dir: PathBuf,
    template: ConversionOptions,
) -> Result<Option<PathBuf>, ForgeError> {
    let fs = StdFileSystem;
    let decoder = ForgeImageDecoder;
    let encoder = ForgeImageEncoder;
    let fit = ResizeStep;
    let transforms: [&dyn forge_core::TransformStep; 1] = [&fit];
    let engine = Orchestrator::new(EngineDeps {
        decoder: &decoder,
        encoder: &encoder,
        transforms: &transforms,
        fs: &fs,
    });
    let options = template.validated()?;
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
