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
        let scale = f64::from(max.width) / f64::from(self.width);
        let scale = scale.min(f64::from(max.height) / f64::from(self.height));
        let width = (f64::from(self.width) * scale).round().max(1.0) as u32;
        let height = (f64::from(self.height) * scale).round().max(1.0) as u32;
        Self { width, height }
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
    /// Optional max bounds; aspect ratio preserved.
    pub max_dimensions: Option<ImageDimensions>,
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
    pub fn validated(self) -> Result<Self> {
        if self.quality == 0 || self.quality > 100 {
            return Err(ForgeError::InvalidConfiguration(format!(
                "quality must be 1-100, got {}",
                self.quality
            )));
        }
        Ok(self)
    }

    /// True when `target` cannot carry alpha (flattening required).
    #[must_use]
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
}
