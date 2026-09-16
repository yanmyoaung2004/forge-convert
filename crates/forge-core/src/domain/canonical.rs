//! Canonical image model (ADR 005): every decoder normalizes here
//! before transforms/encoders. Owned pixels; adapters move (not copy)
//! buffers through the pipeline.

use crate::domain::format::{ColorSpace, PixelFormat};
use crate::domain::options::ImageDimensions;

/// In-memory canonical image: dimensions + layout + owned pixels + tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalImage {
    /// Pixel dimensions.
    pub dimensions: ImageDimensions,
    /// Channel layout of `pixels`.
    pub pixel_format: PixelFormat,
    /// Working color space tag.
    pub color_space: ColorSpace,
    /// Row-major packed pixels (`w*h*bytes_per_pixel` bytes).
    pub pixels: Vec<u8>,
    /// True when an alpha channel survived decode (informational;
    /// encoders decide flatten vs preserve per target).
    pub has_alpha: bool,
}

impl CanonicalImage {
    /// Expected buffer length for the geometry.
    #[must_use]
    pub fn expected_len(dimensions: ImageDimensions, pixel_format: PixelFormat) -> usize {
        dimensions.width as usize * dimensions.height as usize * pixel_format.bytes_per_pixel()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expected_len_matches_buffer() {
        let dims = ImageDimensions::new(4, 2).unwrap();
        assert_eq!(
            CanonicalImage::expected_len(dims, PixelFormat::Rgba8),
            4 * 2 * 4
        );
    }
}
