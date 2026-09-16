//! Named presets: plain [`ConversionOptions`] values with an output
//! format (config, not branches — spec §22).

use crate::domain::format::ImageFormat;
use crate::domain::options::{ConversionOptions, MetadataPolicy};

/// A saved conversion preset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    /// Display name (`"Web Optimized"`).
    pub name: String,
    /// Target format.
    pub output_format: ImageFormat,
    /// Conversion knobs.
    pub options: ConversionOptions,
}

impl Preset {
    /// Flagship web preset: WebP, quality 80, metadata stripped,
    /// dimensions preserved (spec §18).
    #[must_use]
    pub fn web_optimized() -> Self {
        Self {
            name: "Web Optimized".to_string(),
            output_format: ImageFormat::Webp,
            options: ConversionOptions {
                quality: 80,
                metadata: MetadataPolicy::Remove,
                ..Default::default()
            },
        }
    }
    /// High-fidelity JPEG: quality 92, metadata preserved.
    #[must_use]
    pub fn high_quality_jpeg() -> Self {
        Self {
            name: "High Quality JPEG".to_string(),
            output_format: ImageFormat::Jpeg,
            options: ConversionOptions {
                quality: 92,
                ..Default::default()
            },
        }
    }

    /// Small email-friendly JPEG: quality 60, capped at 1600px, stripped.
    #[must_use]
    pub fn small_jpeg() -> Self {
        use crate::domain::options::ImageDimensions;
        Self {
            name: "Small JPEG".to_string(),
            output_format: ImageFormat::Jpeg,
            options: ConversionOptions {
                quality: 60,
                max_dimensions: Some(
                    ImageDimensions::new(1600, 1600).expect("non-zero preset bounds"),
                ),
                metadata: MetadataPolicy::Remove,
                ..Default::default()
            },
        }
    }

    /// Lossless PNG: alpha preserved, metadata kept.
    #[must_use]
    pub fn lossless_png() -> Self {
        Self {
            name: "Lossless PNG".to_string(),
            output_format: ImageFormat::Png,
            options: ConversionOptions::default(),
        }
    }

    /// WebP optimized: quality 80, capped at 2048px, stripped (spec §23).
    #[must_use]
    pub fn webp_optimized() -> Self {
        use crate::domain::options::ImageDimensions;
        Self {
            name: "WebP Optimized".to_string(),
            output_format: ImageFormat::Webp,
            options: ConversionOptions {
                quality: 80,
                max_dimensions: Some(
                    ImageDimensions::new(2048, 2048).expect("non-zero preset bounds"),
                ),
                metadata: MetadataPolicy::Remove,
                ..Default::default()
            },
        }
    }

    /// Look up a built-in by CLI key (`web`, `hq-jpeg`, `small-jpeg`,
    /// `lossless-png`, `webp`) or display name (case-insensitive).
    #[must_use]
    pub fn by_key(key: &str) -> Option<Self> {
        let normalized = key.trim().to_ascii_lowercase();
        Self::builtins().into_iter().find(|preset| {
            preset.name.to_ascii_lowercase() == normalized || preset.key() == normalized
        })
    }

    /// Short CLI key for this preset.
    #[must_use]
    pub fn key(&self) -> String {
        self.name
            .to_ascii_lowercase()
            .replace("webp optimized", "webp")
            .replace("web optimized", "web")
            .replace("high quality jpeg", "hq-jpeg")
            .replace("small jpeg", "small-jpeg")
            .replace("lossless png", "lossless-png")
            .replace(' ', "-")
    }

    /// Built-in preset catalogue (spec §22 subset; customs in Phase 9 store).
    #[must_use]
    pub fn builtins() -> Vec<Self> {
        vec![
            Self::web_optimized(),
            Self::high_quality_jpeg(),
            Self::small_jpeg(),
            Self::lossless_png(),
            Self::webp_optimized(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_optimized_preset_shape() {
        let preset = Preset::web_optimized();
        assert_eq!(preset.output_format, ImageFormat::Webp);
        assert_eq!(preset.options.quality, 80);
        assert_eq!(preset.options.metadata, MetadataPolicy::Remove);
        assert!(preset.options.validated().is_ok());
    }

    #[test]
    fn test_builtins_all_valid() {
        for preset in Preset::builtins() {
            assert!(preset.options.validated().is_ok(), "{}", preset.name);
        }
    }

    #[test]
    fn test_preset_keys_resolve() {
        for key in ["web", "hq-jpeg", "small-jpeg", "lossless-png", "webp"] {
            assert!(Preset::by_key(key).is_some(), "key {key}");
        }
        assert!(Preset::by_key("Web Optimized").is_some(), "display name");
        assert!(Preset::by_key("nope").is_none());
    }

    #[test]
    fn test_small_jpeg_caps_dimensions() {
        let preset = Preset::small_jpeg();
        assert_eq!(preset.output_format, ImageFormat::Jpeg);
        assert!(preset.options.max_dimensions.is_some());
    }
}
