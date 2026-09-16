//! Conversion options: quality, resize, metadata, background,
//! collisions, page ranges. Validated at construction.

use crate::domain::format::ImageFormat;
use crate::error::{ForgeError, Result};

/// Metadata handling: explicit preserve-or-remove (spec §20 / Master §9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MetadataPolicy {
    /// Keep metadata where the target format supports it.
    #[default]
    Preserve,
    /// Strip metadata (privacy / web optimization).
    Remove,
}

/// Background for alpha-flattening when the target has no alpha channel
/// (JPEG, BMP). Never silently fake transparency (spec §10 / Master §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BackgroundPolicy {
    /// sRGB background as (r, g, b).
    pub rgb: (u8, u8, u8),
}

impl Default for BackgroundPolicy {
    /// Safe, predictable default: white.
    fn default() -> Self {
        Self {
            rgb: (255, 255, 255),
        }
    }
}

/// What to do when the output path already exists (spec §24).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CollisionPolicy {
    /// `stem.webp`, `stem (1).webp`, … — the safe default.
    #[default]
    RenameAuto,
    /// Overwrite the existing file (explicit opt-in only).
    Replace,
    /// Leave the existing file; report the input as skipped.
    Skip,
    /// Fail with [`ForgeError::OutputExists`].
    Fail,
}

/// Pixel dimensions (validated non-zero).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

impl ImageDimensions {
    /// Zero in either axis is [`ForgeError::InvalidConfiguration`].
    pub fn new(width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(ForgeError::InvalidConfiguration(format!(
                "dimensions must be non-zero, got {width}x{height}"
            )));
        }
        Ok(Self { width, height })
    }

    /// Scale to fit inside `max`, preserving aspect ratio. No-op when
    /// already inside bounds or `max` is `None`.
    #[must_use]
    pub fn fit_within(self, max: Option<Self>) -> Self {
        let Some(max) = max else { return self };
        if self.width <= max.width && self.height <= max.height {
            return self;
        }
        self.scaled_to_fit(max)
    }

    /// Scale factor to fit inside `bounds` (aspect kept, may upscale).
    fn scale_to_fit(self, bounds: Self) -> f64 {
        (f64::from(bounds.width) / f64::from(self.width))
            .min(f64::from(bounds.height) / f64::from(self.height))
    }

    /// Dimensions after fit-inside scaling (aspect kept, may upscale).
    #[must_use]
    pub fn scaled_to_fit(self, bounds: Self) -> Self {
        let scale = self.scale_to_fit(bounds);
        let width = (f64::from(self.width) * scale).round().max(1.0) as u32;
        let height = (f64::from(self.height) * scale).round().max(1.0) as u32;
        Self { width, height }
    }

    /// Dimensions after cover scaling (aspect kept, then crop to bounds).
    #[must_use]
    pub fn scaled_to_cover(self, bounds: Self) -> Self {
        let scale = (f64::from(bounds.width) / f64::from(self.width))
            .max(f64::from(bounds.height) / f64::from(self.height));
        let width = (f64::from(self.width) * scale).round().max(1.0) as u32;
        let height = (f64::from(self.height) * scale).round().max(1.0) as u32;
        Self { width, height }
    }

    /// Center-crop box of `target` inside `self` (assumes `self >= target`
    /// per axis; clamps defensively so adapters never panic).
    #[must_use]
    pub fn center_crop_origin(self, target: Self) -> (u32, u32) {
        (
            self.width.saturating_sub(target.width) / 2,
            self.height.saturating_sub(target.height) / 2,
        )
    }
}

/// How to resample when resizing (maps to `image::imageops::FilterType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ResizeFilter {
    /// Fast, smooth enough for downscales (default).
    #[default]
    Lanczos3,
    /// Smooth bicubic-ish (CatmullRom).
    CatmullRom,
    /// Soft gaussian blur-ish.
    Gaussian,
    /// Blocky but fastest (pixel art, masks).
    Nearest,
}

/// Explicit resize request: exact `width`×`height` wins; otherwise fit/fill.
///
/// Semantics (all preserve correctness, never stretch silently):
/// - `Exact`: force exact dimensions (may change aspect — explicit opt-in).
/// - `Fit`: scale down (or up, if `upscale`) to fit INSIDE the box, aspect kept.
/// - `Fill`: scale to COVER the box then center-crop, aspect kept.
/// - `None` (no `ResizeSpec`): keep source dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResizeSpec {
    /// Force exact output size (aspect may change).
    Exact {
        dimensions: ImageDimensions,
        filter: ResizeFilter,
    },
    /// Fit inside the box, aspect preserved. `upscale: false` never enlarges.
    Fit {
        bounds: ImageDimensions,
        filter: ResizeFilter,
        upscale: bool,
    },
    /// Cover the box then center-crop, aspect preserved.
    Fill {
        bounds: ImageDimensions,
        filter: ResizeFilter,
        upscale: bool,
    },
}

impl ResizeSpec {
    /// Validate bounds (non-zero guaranteed by `ImageDimensions::new`).
    /// Exact with equal dims is a no-op at apply time, not an error.
    #[must_use]
    pub fn filter(self) -> ResizeFilter {
        match self {
            Self::Exact { filter, .. } | Self::Fit { filter, .. } | Self::Fill { filter, .. } => {
                filter
            }
        }
    }
}

/// Per-format compression tuning. Precedence: when `compression` is set,
/// its embedded quality wins for that format; otherwise `quality` applies.
/// `None` (= today's behavior): PNG default level, JPEG/WebP at `quality`.
/// Mismatched variants are ignored (e.g. `Png{..}` for a JPEG target
/// falls back to `quality`) — never an error, never silent magic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Compression {
    /// PNG: level 0 (fast) – 9 (smallest) + adaptive filtering.
    /// Maps to `png::CompressionType::Default/Fast/Best`.
    Png { level: u8 },
    /// JPEG: quality 1–100 (validated against `quality` when both set:
    /// explicit `quality` field wins; this mirrors it for clarity).
    Jpeg { quality: u8 },
    /// WebP lossy quality 1–100 (libwebp `encode(q)`).
    WebpLossy { quality: u8 },
    /// WebP lossless (VP8L via `image-webp`).
    WebpLossless,
}

impl Default for Compression {
    fn default() -> Self {
        Self::WebpLossy { quality: 80 }
    }
}
/// Descriptive metadata surfaced by inspection (spec §19 / Master §24).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImageMetadata {
    /// Whether any metadata chunks were observed (EXIF/ICC/XMP/PNG text).
    pub has_metadata: bool,
    /// Raw EXIF blob when available (opaque to the core).
    pub exif: Option<Vec<u8>>,
}

/// Page selection for PDF→image (1-based, inclusive).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PageRange {
    /// Sorted, deduplicated 1-based page numbers.
    pub pages: Vec<u32>,
}

impl PageRange {
    /// Parse `"3"`, `"1-3"`, `"1,3,5-7"` (whitespace tolerated).
    /// Empty segments, zero pages, and `start > end` are errors.
    pub fn parse(spec: &str) -> Result<Self> {
        let mut pages: Vec<u32> = Vec::new();
        for part in spec.split(',') {
            let part = part.trim();
            if part.is_empty() {
                return Err(ForgeError::InvalidConfiguration(
                    "empty page in page range".to_string(),
                ));
            }
            if let Some((a, b)) = part.split_once('-') {
                let start: u32 = a
                    .trim()
                    .parse()
                    .map_err(|_| ForgeError::InvalidConfiguration(format!("bad page: {a}")))?;
                let end: u32 = b
                    .trim()
                    .parse()
                    .map_err(|_| ForgeError::InvalidConfiguration(format!("bad page: {b}")))?;
                if start == 0 || end == 0 || start > end {
                    return Err(ForgeError::InvalidConfiguration(format!(
                        "bad page range: {part}"
                    )));
                }
                pages.extend(start..=end);
            } else {
                let page: u32 = part
                    .parse()
                    .map_err(|_| ForgeError::InvalidConfiguration(format!("bad page: {part}")))?;
                if page == 0 {
                    return Err(ForgeError::InvalidConfiguration(
                        "page numbers start at 1".to_string(),
                    ));
                }
                pages.push(page);
            }
        }
        if pages.is_empty() {
            return Err(ForgeError::InvalidConfiguration(
                "empty page range".to_string(),
            ));
        }
        pages.sort_unstable();
        pages.dedup();
        Ok(Self { pages })
    }
}

/// Per-conversion knobs. Validated at construction; presets are plain
/// `ConversionOptions` values (config, not branches — spec §22).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionOptions {
    /// 1–100. Meaningful for lossy targets (JPEG, lossy WebP).
    pub quality: u8,
    /// Legacy max bounds (fit-inside, aspect kept, never upscale).
    /// Prefer `resize`; when both are set, `resize` wins.
    pub max_dimensions: Option<ImageDimensions>,
    /// Explicit resize (exact / fit / fill + filter). `None` = keep size.
    pub resize: Option<ResizeSpec>,
    /// Per-format compression tuning. `None` = today's defaults
    /// (PNG default level, JPEG/WebP at `quality`).
    pub compression: Option<Compression>,
    /// Preserve or strip metadata.
    pub metadata: MetadataPolicy,
    /// Background when flattening alpha onto alpha-less targets.
    pub background: BackgroundPolicy,
    /// Existing-output behavior.
    pub on_collision: CollisionPolicy,
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            quality: 80,
            max_dimensions: None,
            resize: None,
            compression: None,
            metadata: MetadataPolicy::default(),
            background: BackgroundPolicy::default(),
            on_collision: CollisionPolicy::default(),
        }
    }
}

impl ConversionOptions {
    /// Quality must be 1–100; RGBA→alpha-less-target is fine because
    /// `background` always supplies the flatten color (default white).
    /// Rejects quality 0 and >100 with [`ForgeError::InvalidConfiguration`].
    /// Compression-embedded qualities (JPEG/WebP/PNG-level) are validated too.
    pub fn validated(self) -> Result<Self> {
        if self.quality == 0 || self.quality > 100 {
            return Err(ForgeError::InvalidConfiguration(format!(
                "quality must be 1-100, got {}",
                self.quality
            )));
        }
        match self.compression {
            Some(Compression::Jpeg { quality } | Compression::WebpLossy { quality })
                if quality == 0 || quality > 100 =>
            {
                return Err(ForgeError::InvalidConfiguration(format!(
                    "compression quality must be 1-100, got {quality}"
                )));
            }
            Some(Compression::Png { level }) if level > 9 => {
                return Err(ForgeError::InvalidConfiguration(format!(
                    "PNG compression level must be 0-9, got {level}"
                )));
            }
            _ => {}
        }
        Ok(self)
    }
    pub fn requires_flattening(target: ImageFormat) -> bool {
        !target.descriptor().supports_alpha
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_bounds_rejected() {
        for bad in [0, 101, 255] {
            let opts = ConversionOptions {
                quality: bad,
                ..Default::default()
            };
            assert!(
                matches!(opts.validated(), Err(ForgeError::InvalidConfiguration(_))),
                "quality {bad}"
            );
        }
        assert!(ConversionOptions::default().validated().is_ok());
    }

    #[test]
    fn test_dimensions_reject_zero() {
        assert!(ImageDimensions::new(0, 10).is_err());
        assert!(ImageDimensions::new(10, 0).is_err());
        assert!(ImageDimensions::new(800, 600).is_ok());
    }

    #[test]
    fn test_fit_within_preserves_ratio() {
        let big = ImageDimensions::new(2000, 1000).unwrap();
        let max = ImageDimensions::new(800, 800).unwrap();
        assert_eq!(
            big.fit_within(Some(max)),
            ImageDimensions::new(800, 400).unwrap()
        );
        let small = ImageDimensions::new(100, 100).unwrap();
        assert_eq!(small.fit_within(Some(max)), small);
        assert_eq!(big.fit_within(None), big);
    }

    #[test]
    fn test_page_range_parse() {
        assert_eq!(PageRange::parse("3").unwrap().pages, vec![3]);
        assert_eq!(PageRange::parse("1-3").unwrap().pages, vec![1, 2, 3]);
        assert_eq!(PageRange::parse("1,3,5-6").unwrap().pages, vec![1, 3, 5, 6]);
        assert_eq!(
            PageRange::parse("3,1-2").unwrap().pages,
            vec![1, 2, 3],
            "sorted"
        );
        for bad in ["", "0", "3-1", "a", "1,,2", "1-"] {
            assert!(PageRange::parse(bad).is_err(), "range {bad:?}");
        }
    }

    #[test]
    fn test_flattening_matrix() {
        assert!(ConversionOptions::requires_flattening(ImageFormat::Jpeg));
        assert!(ConversionOptions::requires_flattening(ImageFormat::Bmp));
        assert!(!ConversionOptions::requires_flattening(ImageFormat::Png));
        assert!(!ConversionOptions::requires_flattening(ImageFormat::Webp));
    }

    #[test]
    fn test_background_default_is_white() {
        assert_eq!(
            BackgroundPolicy::default().rgb,
            (255, 255, 255),
            "safe predictable default per Master Spec §10"
        );
    }

    #[test]
    fn test_scaled_geometries() {
        let src = ImageDimensions::new(1020, 900).unwrap();
        // Fit 800×600: scale = min(800/1020, 600/900) = 2/3 → 680×600.
        assert_eq!(
            src.scaled_to_fit(ImageDimensions::new(800, 600).unwrap()),
            ImageDimensions::new(680, 600).unwrap()
        );
        // Cover 800×600: scale = max(800/1020, 600/900) ≈ 0.784 → 800×706.
        assert_eq!(
            src.scaled_to_cover(ImageDimensions::new(800, 600).unwrap()),
            ImageDimensions::new(800, 706).unwrap()
        );
        // Crop origin centers the 800×600 box in the 800×706 cover.
        assert_eq!(
            ImageDimensions::new(800, 706)
                .unwrap()
                .center_crop_origin(ImageDimensions::new(800, 600).unwrap()),
            (0, 53)
        );
    }

    #[test]
    fn test_compression_bounds_validated() {
        let ok = ConversionOptions {
            compression: Some(Compression::Png { level: 9 }),
            ..Default::default()
        };
        assert!(ok.validated().is_ok());
        for bad in [
            Compression::Png { level: 10 },
            Compression::Jpeg { quality: 0 },
            Compression::WebpLossy { quality: 101 },
        ] {
            let opts = ConversionOptions {
                compression: Some(bad),
                ..Default::default()
            };
            assert!(
                matches!(opts.validated(), Err(ForgeError::InvalidConfiguration(_))),
                "{bad:?}"
            );
        }
    }
}
