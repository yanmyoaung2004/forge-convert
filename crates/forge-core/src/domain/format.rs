//! Image formats, pixel types, and capability discovery.
//!
//! Pure data + magic-byte detection. No I/O, no codecs.

use crate::error::{ForgeError, Result};

/// Image/document formats supported (or planned) by ForgeConvert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
    Bmp,
    Tiff,
    Pdf,
}

impl ImageFormat {
    /// Canonical file extension (no dot).
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Bmp => "bmp",
            Self::Tiff => "tiff",
            Self::Pdf => "pdf",
        }
    }

    /// Canonical MIME type.
    #[must_use]
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
            Self::Bmp => "image/bmp",
            Self::Tiff => "image/tiff",
            Self::Pdf => "application/pdf",
        }
    }

    /// Parse from an extension (dot optional, case-insensitive).
    /// `jpg` and `jpeg` both map to JPEG; `tif` maps to TIFF.
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "webp" => Some(Self::Webp),
            "bmp" => Some(Self::Bmp),
            "tif" | "tiff" => Some(Self::Tiff),
            "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }

    /// Static capability descriptor for UI/CLI queries.
    #[must_use]
    pub const fn descriptor(self) -> FormatDescriptor {
        match self {
            Self::Png => FormatDescriptor {
                name: "PNG",
                extensions: &["png"],
                mime_types: &["image/png"],
                can_decode: true,
                can_encode: true,
                supports_alpha: true,
                supports_lossless: true,
                supports_lossy: false,
                supports_metadata: true,
            },
            Self::Jpeg => FormatDescriptor {
                name: "JPEG",
                extensions: &["jpg", "jpeg"],
                mime_types: &["image/jpeg"],
                can_decode: true,
                can_encode: true,
                supports_alpha: false,
                supports_lossless: false,
                supports_lossy: true,
                supports_metadata: true,
            },
            Self::Webp => FormatDescriptor {
                name: "WebP",
                extensions: &["webp"],
                mime_types: &["image/webp"],
                can_decode: true,
                can_encode: true,
                supports_alpha: true,
                supports_lossless: true,
                supports_lossy: true,
                supports_metadata: true,
            },
            Self::Bmp => FormatDescriptor {
                name: "BMP",
                extensions: &["bmp"],
                mime_types: &["image/bmp"],
                can_decode: true,
                can_encode: true,
                supports_alpha: false,
                supports_lossless: true,
                supports_lossy: false,
                supports_metadata: false,
            },
            Self::Tiff => FormatDescriptor {
                name: "TIFF",
                extensions: &["tif", "tiff"],
                mime_types: &["image/tiff"],
                can_decode: true,
                can_encode: true,
                supports_alpha: true,
                supports_lossless: true,
                supports_lossy: false,
                supports_metadata: true,
            },
            Self::Pdf => FormatDescriptor {
                name: "PDF",
                extensions: &["pdf"],
                mime_types: &["application/pdf"],
                can_decode: false, // renderer deferred (ADR 009/012)
                can_encode: true,  // image→PDF via printpdf
                supports_alpha: false,
                supports_lossless: true,
                supports_lossy: false,
                supports_metadata: true,
            },
        }
    }

    /// All formats in stable order (UI lists, tests).
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Png,
            Self::Jpeg,
            Self::Webp,
            Self::Bmp,
            Self::Tiff,
            Self::Pdf,
        ]
    }
}

/// Static capability record surfaced to UI/CLI
/// ("what can I convert this file to?").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatDescriptor {
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub mime_types: &'static [&'static str],
    pub can_decode: bool,
    pub can_encode: bool,
    pub supports_alpha: bool,
    pub supports_lossless: bool,
    pub supports_lossy: bool,
    pub supports_metadata: bool,
}

/// Capability queries over [`ImageFormat`].
pub struct FormatCapabilities;

impl FormatCapabilities {
    /// Valid conversion targets for `source`.
    ///
    /// Images convert to images + PDF; PDF converts to images
    /// (render port is stubbed → runtime `Unsupported` until qualified).
    #[must_use]
    pub fn targets_for(source: ImageFormat) -> Vec<ImageFormat> {
        match source {
            ImageFormat::Pdf => vec![ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Webp],
            _ => vec![
                ImageFormat::Png,
                ImageFormat::Jpeg,
                ImageFormat::Webp,
                ImageFormat::Bmp,
                ImageFormat::Tiff,
                ImageFormat::Pdf,
            ]
            .into_iter()
            .filter(|f| *f != source)
            .collect(),
        }
    }

    /// Descriptors for every known format.
    #[must_use]
    pub fn all_descriptors() -> Vec<FormatDescriptor> {
        ImageFormat::all().iter().map(|f| f.descriptor()).collect()
    }
}

/// Pixel layout of a [`crate::domain::CanonicalImage`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// 8-bit grayscale.
    Luma8,
    /// 8-bit grayscale + alpha.
    La8,
    /// 8-bit RGB.
    Rgb8,
    /// 8-bit RGBA.
    Rgba8,
}

impl PixelFormat {
    /// Bytes per pixel.
    #[must_use]
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Luma8 => 1,
            Self::La8 => 2,
            Self::Rgb8 => 3,
            Self::Rgba8 => 4,
        }
    }

    /// Whether this layout carries an alpha channel.
    #[must_use]
    pub const fn has_alpha(self) -> bool {
        matches!(self, Self::La8 | Self::Rgba8)
    }
}

/// Working color space tag (no ICC math in MVP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColorSpace {
    /// Default for decoded images.
    #[default]
    Srgb,
    /// Explicit linear-light working space (future).
    Linear,
    /// Source space unknown / untagged.
    Unknown,
}

/// Detect format from magic bytes (no filesystem access).
///
/// Returns [`ForgeError::InvalidFile`] for empty input,
/// [`ForgeError::UnsupportedFormat`] when no signature matches.
pub fn detect_format(bytes: &[u8]) -> Result<ImageFormat> {
    if bytes.is_empty() {
        return Err(ForgeError::InvalidFile("empty input".to_string()));
    }
    // PNG: 89 50 4E 47 0D 0A 1A 0A
    if bytes.len() >= 8 && bytes[0..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return Ok(ImageFormat::Png);
    }
    // JPEG: FF D8 FF
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return Ok(ImageFormat::Jpeg);
    }
    // WebP: RIFF xxxx WEBP
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Ok(ImageFormat::Webp);
    }
    // BMP: 42 4D ("BM")
    if bytes.len() >= 2 && bytes[0] == 0x42 && bytes[1] == 0x4D {
        return Ok(ImageFormat::Bmp);
    }
    // TIFF: II*\0 (little) or MM\0* (big)
    if bytes.len() >= 4
        && ((bytes[0] == 0x49 && bytes[1] == 0x49 && bytes[2] == 0x2A && bytes[3] == 0x00)
            || (bytes[0] == 0x4D && bytes[1] == 0x4D && bytes[2] == 0x00 && bytes[3] == 0x2A))
    {
        return Ok(ImageFormat::Tiff);
    }
    // PDF: %PDF
    if bytes.len() >= 4 && &bytes[0..4] == b"%PDF" {
        return Ok(ImageFormat::Pdf);
    }
    Err(ForgeError::UnsupportedFormat(
        "unrecognized file signature".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_roundtrip() {
        for f in ImageFormat::all() {
            let ext = f.extension();
            let back = ImageFormat::from_extension(ext);
            assert_eq!(back, Some(*f), "extension {ext}");
        }
        assert_eq!(ImageFormat::from_extension("JPG"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_extension(".tif"), Some(ImageFormat::Tiff));
        assert_eq!(ImageFormat::from_extension("xyz"), None);
    }

    #[test]
    fn test_detect_png_signature() {
        let png = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
        assert_eq!(detect_format(&png), Ok(ImageFormat::Png));
    }

    #[test]
    fn test_detect_jpeg_signature() {
        assert_eq!(
            detect_format(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]),
            Ok(ImageFormat::Jpeg)
        );
    }

    #[test]
    fn test_detect_webp_signature() {
        let mut riff = b"RIFF".to_vec();
        riff.extend_from_slice(&[0x10, 0x00, 0x00, 0x00]);
        riff.extend_from_slice(b"WEBP");
        assert_eq!(detect_format(&riff), Ok(ImageFormat::Webp));
    }

    #[test]
    fn test_detect_bmp_tiff_pdf_signatures() {
        assert_eq!(detect_format(&[0x42, 0x4D, 0x00]), Ok(ImageFormat::Bmp));
        assert_eq!(
            detect_format(&[0x49, 0x49, 0x2A, 0x00]),
            Ok(ImageFormat::Tiff)
        );
        assert_eq!(
            detect_format(&[0x4D, 0x4D, 0x00, 0x2A]),
            Ok(ImageFormat::Tiff)
        );
        assert_eq!(detect_format(b"%PDF-1.7"), Ok(ImageFormat::Pdf));
    }

    #[test]
    fn test_detect_empty_is_invalid_not_unsupported() {
        assert!(matches!(
            detect_format(&[]),
            Err(ForgeError::InvalidFile(_))
        ));
        assert!(matches!(
            detect_format(b"nope-nope-nope"),
            Err(ForgeError::UnsupportedFormat(_))
        ));
    }

    #[test]
    fn test_capabilities_exclude_self_and_cover_pdf() {
        for f in ImageFormat::all() {
            let targets = FormatCapabilities::targets_for(*f);
            assert!(!targets.contains(f), "self target for {f:?}");
            assert!(!targets.is_empty());
        }
        // Images can target PDF; PDF targets images only.
        assert!(FormatCapabilities::targets_for(ImageFormat::Png).contains(&ImageFormat::Pdf));
        assert!(!FormatCapabilities::targets_for(ImageFormat::Pdf).contains(&ImageFormat::Pdf));
    }

    #[test]
    fn test_pixel_format_alpha_and_bpp() {
        assert!(PixelFormat::Rgba8.has_alpha());
        assert!(PixelFormat::La8.has_alpha());
        assert!(!PixelFormat::Rgb8.has_alpha());
        assert_eq!(PixelFormat::Rgba8.bytes_per_pixel(), 4);
        assert_eq!(PixelFormat::Luma8.bytes_per_pixel(), 1);
    }
}
