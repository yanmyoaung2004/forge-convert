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

    /// Built-in preset catalogue (Phase 8 extends with stored customs).
    #[must_use]
    pub fn builtins() -> Vec<Self> {
        vec![Self::web_optimized(), Self::high_quality_jpeg()]
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
}
