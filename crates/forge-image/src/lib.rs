//! Image adapter: decode → canonical → transform → encode.
//!
//! - Decode PNG/JPEG/BMP/TIFF/WebP via `image`.
//! - Encode PNG/JPEG/BMP/TIFF via `image`; lossy WebP via `webp`
//!   (ADR 012: `image-webp` is VP8L lossless-only).
//! - RGBA → alpha-less targets flatten over [`BackgroundPolicy`] first.

use forge_core::{
    BackgroundPolicy, CanonicalImage, ColorSpace, ConversionOptions, ImageDimensions, ImageFormat,
    PixelFormat, Result,
};
use forge_core::{ForgeError, ImageDecoder, ImageEncoder, TransformStep};
#[derive(Debug, Default)]
pub struct ForgeImageDecoder;

impl ForgeImageDecoder {
    /// Supported input formats.
    pub const INPUTS: &'static [ImageFormat] = &[
        ImageFormat::Png,
        ImageFormat::Jpeg,
        ImageFormat::Webp,
        ImageFormat::Bmp,
        ImageFormat::Tiff,
    ];
}

impl ImageDecoder for ForgeImageDecoder {
    fn supported_inputs(&self) -> &'static [ImageFormat] {
        Self::INPUTS
    }

    fn decode(&self, bytes: &[u8], hint: Option<ImageFormat>) -> Result<CanonicalImage> {
        if bytes.is_empty() {
            return Err(ForgeError::InvalidFile("empty input".to_string()));
        }
        // Content sniff wins; fall back to the caller hint.
        let format = match forge_core::detect_format(bytes) {
            Ok(format) => format,
            Err(_) => hint.ok_or_else(|| {
                ForgeError::UnsupportedFormat("unrecognized file signature".to_string())
            })?,
        };
        if !Self::INPUTS.contains(&format) {
            return Err(ForgeError::UnsupportedFormat(format!(
                "image decoder cannot read {}",
                format.mime_type()
            )));
        }
        let image_format = to_image_format(format).ok_or_else(|| {
            ForgeError::UnsupportedFormat(format!("no image mapping for {format:?}"))
        })?;
        let dyn_image = image::load_from_memory_with_format(bytes, image_format)
            .map_err(|e| ForgeError::DecodeFailed(e.to_string()))?;
        Ok(canonicalize(&dyn_image))
    }
}

/// All-format encoder: `image` + lossy `webp`.
#[derive(Debug, Default)]
pub struct ForgeImageEncoder;

/// Quality slider 1–100 → libwebp `encode(quality: f32)` 0–100 scale.
const WEBP_QUALITY_SCALE: f32 = 1.0;

impl ForgeImageEncoder {
    /// Supported output formats.
    pub const OUTPUTS: &'static [ImageFormat] = &[
        ImageFormat::Png,
        ImageFormat::Jpeg,
        ImageFormat::Webp,
        ImageFormat::Bmp,
        ImageFormat::Tiff,
    ];
}

impl ImageEncoder for ForgeImageEncoder {
    fn supported_outputs(&self) -> &'static [ImageFormat] {
        Self::OUTPUTS
    }

    fn encode(
        &self,
        image: &CanonicalImage,
        target: ImageFormat,
        options: &ConversionOptions,
    ) -> Result<Vec<u8>> {
        if !Self::OUTPUTS.contains(&target) {
            return Err(ForgeError::UnsupportedFormat(format!(
                "image encoder cannot write {}",
                target.mime_type()
            )));
        }
        // Flatten alpha onto alpha-less targets BEFORE encoding (spec §10):
        // JPEG/BMP encoders must never see RGBA.
        let owned;
        let image = if options_flatten_needed(image, target) {
            owned = flatten(image, options.background);
            &owned
        } else {
            image
        };
        match target {
            ImageFormat::Webp => encode_webp_lossy(image, options.quality),
            ImageFormat::Png => encode_png(image),
            ImageFormat::Jpeg => encode_jpeg(image, options.quality),
            ImageFormat::Bmp | ImageFormat::Tiff => encode_via_image(image, target),
            ImageFormat::Pdf => Err(ForgeError::UnsupportedFormat(
                "PDF is written by forge-pdf, not the image encoder".to_string(),
            )),
        }
    }
}

/// Resize step: fit inside `options.max_dimensions`, Lanczos3.
#[derive(Debug, Default)]
pub struct FitWithinStep;

impl TransformStep for FitWithinStep {
    fn name(&self) -> &'static str {
        "fit-within"
    }

    fn apply(&self, image: CanonicalImage, options: &ConversionOptions) -> Result<CanonicalImage> {
        let Some(max) = options.max_dimensions else {
            return Ok(image);
        };
        let target = image.dimensions.fit_within(Some(max));
        if target == image.dimensions {
            return Ok(image);
        }
        resize_canonical(&image, target)
    }
}

/// Convert `ImageFormat` → `image::ImageFormat` (PDF has no mapping).
fn to_image_format(format: ImageFormat) -> Option<image::ImageFormat> {
    match format {
        ImageFormat::Png => Some(image::ImageFormat::Png),
        ImageFormat::Jpeg => Some(image::ImageFormat::Jpeg),
        ImageFormat::Webp => Some(image::ImageFormat::WebP),
        ImageFormat::Bmp => Some(image::ImageFormat::Bmp),
        ImageFormat::Tiff => Some(image::ImageFormat::Tiff),
        ImageFormat::Pdf => None,
    }
}

/// `DynamicImage` → canonical RGBA8/RGB8/Luma model.
fn canonicalize(img: &image::DynamicImage) -> CanonicalImage {
    use image::DynamicImage;
    let (pixels, pixel_format, has_alpha) = match img {
        DynamicImage::ImageRgba8(buf) => (buf.as_raw().clone(), PixelFormat::Rgba8, true),
        DynamicImage::ImageRgb8(buf) => (buf.as_raw().clone(), PixelFormat::Rgb8, false),
        DynamicImage::ImageLuma8(buf) => (buf.as_raw().clone(), PixelFormat::Luma8, false),
        DynamicImage::ImageLumaA8(buf) => (buf.as_raw().clone(), PixelFormat::La8, true),
        // Normalize wide/float buffers through 8-bit RGBA (MVP scope).
        other => (other.to_rgba8().into_raw(), PixelFormat::Rgba8, true),
    };
    let dimensions = ImageDimensions::new(img.width(), img.height())
        .expect("decoded image has non-zero dimensions");
    CanonicalImage {
        dimensions,
        pixel_format,
        color_space: ColorSpace::Srgb,
        pixels,
        has_alpha,
    }
}

/// Canonical → `DynamicImage` for `image`-crate encoders/ops.
fn to_dynamic(image: &CanonicalImage) -> Result<image::DynamicImage> {
    use image::{RgbImage, RgbaImage};
    let expected = CanonicalImage::expected_len(image.dimensions, image.pixel_format);
    if image.pixels.len() != expected {
        return Err(ForgeError::InvalidFile(format!(
            "pixel buffer mismatch: got {}, want {expected}",
            image.pixels.len()
        )));
    }
    let (w, h) = (image.dimensions.width, image.dimensions.height);
    match image.pixel_format {
        PixelFormat::Rgba8 => RgbaImage::from_raw(w, h, image.pixels.clone())
            .map(image::DynamicImage::ImageRgba8)
            .ok_or_else(|| ForgeError::InvalidFile("bad RGBA buffer".to_string())),
        PixelFormat::Rgb8 => RgbImage::from_raw(w, h, image.pixels.clone())
            .map(image::DynamicImage::ImageRgb8)
            .ok_or_else(|| ForgeError::InvalidFile("bad RGB buffer".to_string())),
        // Expand luma layouts to RGBA for encoder/ops uniformity (MVP scope).
        PixelFormat::Luma8 => {
            let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
            for &l in &image.pixels {
                rgba.extend_from_slice(&[l, l, l, 255]);
            }
            RgbaImage::from_raw(w, h, rgba)
                .map(image::DynamicImage::ImageRgba8)
                .ok_or_else(|| ForgeError::InvalidFile("bad Luma buffer".to_string()))
        }
        PixelFormat::La8 => {
            let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
            let (pairs, _) = image.pixels.as_chunks::<2>();
            for pair in pairs {
                rgba.extend_from_slice(&[pair[0], pair[0], pair[0], pair[1]]);
            }
            RgbaImage::from_raw(w, h, rgba)
                .map(image::DynamicImage::ImageRgba8)
                .ok_or_else(|| ForgeError::InvalidFile("bad La buffer".to_string()))
        }
    }
}

/// True when the target drops alpha and the image carries it.
fn options_flatten_needed(image: &CanonicalImage, target: ImageFormat) -> bool {
    !target.descriptor().supports_alpha && (image.pixel_format.has_alpha() || image.has_alpha)
}

/// Alpha-composite over `background` → opaque RGB8.
fn flatten(image: &CanonicalImage, background: BackgroundPolicy) -> CanonicalImage {
    let dyn_image = to_dynamic(image).expect("flatten input is canonical");
    let rgba = dyn_image.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let (br, bg, bb) = background.rgb;
    let mut rgb = Vec::with_capacity(w as usize * h as usize * 3);
    for px in rgba.pixels() {
        let a = f32::from(px[3]) / 255.0;
        let mix = |fg: u8, b: u8| (f32::from(fg) * a + f32::from(b) * (1.0 - a)).round() as u8;
        rgb.extend_from_slice(&[mix(px[0], br), mix(px[1], bg), mix(px[2], bb)]);
    }
    CanonicalImage {
        dimensions: image.dimensions,
        pixel_format: PixelFormat::Rgb8,
        color_space: image.color_space,
        pixels: rgb,
        has_alpha: false,
    }
}

/// Lanczos3 resize to `target` (aspect already fitted by caller).
fn resize_canonical(image: &CanonicalImage, target: ImageDimensions) -> Result<CanonicalImage> {
    let dyn_image = to_dynamic(image)?;
    let resized = dyn_image.resize_exact(
        target.width,
        target.height,
        image::imageops::FilterType::Lanczos3,
    );
    let rgba = resized.to_rgba8();
    Ok(CanonicalImage {
        dimensions: target,
        pixel_format: PixelFormat::Rgba8,
        color_space: image.color_space,
        pixels: rgba.into_raw(),
        has_alpha: true,
    })
}

/// PNG via `image` (default compression; `oxipng` later in Phase 8).
/// Preserves alpha: RGBA/La in → RGBA out, others → RGB out.
fn encode_png(image: &CanonicalImage) -> Result<Vec<u8>> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::{ColorType, ExtendedColorType};
    let dyn_image = to_dynamic(image)?;
    // Own the encoded-from buffer so the borrowed slice outlives the call.
    enum Owned {
        Rgba(Vec<u8>),
        Rgb(Vec<u8>),
    }
    let owned = match image.pixel_format {
        PixelFormat::Rgba8 | PixelFormat::La8 => Owned::Rgba(dyn_image.to_rgba8().into_raw()),
        _ => Owned::Rgb(dyn_image.to_rgb8().into_raw()),
    };
    let (bytes, color): (&[u8], ExtendedColorType) = match &owned {
        Owned::Rgba(buf) => (buf, ColorType::Rgba8.into()),
        Owned::Rgb(buf) => (buf, ColorType::Rgb8.into()),
    };
    let mut out = Vec::new();
    {
        use image::ImageEncoder as _;
        let encoder =
            PngEncoder::new_with_quality(&mut out, CompressionType::Default, FilterType::Adaptive);
        encoder
            .write_image(
                bytes,
                image.dimensions.width,
                image.dimensions.height,
                color,
            )
            .map_err(|e| ForgeError::EncodeFailed(e.to_string()))?;
    }
    Ok(out)
}

/// JPEG via `image` with explicit quality 1–100.
fn encode_jpeg(image: &CanonicalImage, quality: u8) -> Result<Vec<u8>> {
    use image::codecs::jpeg::JpegEncoder;
    let dyn_image = to_dynamic(image)?;
    let mut out = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut out, quality);
    encoder
        .encode_image(&dyn_image)
        .map_err(|e| ForgeError::EncodeFailed(e.to_string()))?;
    Ok(out)
}

/// Lossy WebP via `libwebp` wrapper (ADR 012). Quality 1–100 → 0–100 float.
fn encode_webp_lossy(image: &CanonicalImage, quality: u8) -> Result<Vec<u8>> {
    let dyn_image = to_dynamic(image)?;
    let rgba = dyn_image.to_rgba8();
    let encoder = webp::Encoder::from_rgba(
        rgba.as_raw(),
        image.dimensions.width,
        image.dimensions.height,
    );
    let quality = f32::from(quality.clamp(1, 100)) * WEBP_QUALITY_SCALE;
    Ok(encoder.encode(quality).to_vec())
}

/// BMP/TIFF via `image` guessing from raw pixels.
fn encode_via_image(image: &CanonicalImage, target: ImageFormat) -> Result<Vec<u8>> {
    let dyn_image = to_dynamic(image)?;
    let mut out = Vec::new();
    let format = to_image_format(target)
        .ok_or_else(|| ForgeError::UnsupportedFormat(format!("no image mapping for {target:?}")))?;
    dyn_image
        .write_to(&mut std::io::Cursor::new(&mut out), format)
        .map_err(|e| ForgeError::EncodeFailed(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_core::ConversionOptions;

    /// 4×3 RGBA gradient fixture (generated — no binaries committed).
    fn rgba_fixture() -> CanonicalImage {
        let dims = ImageDimensions::new(4, 3).unwrap();
        let mut pixels = Vec::new();
        for y in 0..3 {
            for x in 0..4 {
                pixels.extend_from_slice(&[x * 60, y * 80, 128, 200]);
            }
        }
        CanonicalImage {
            dimensions: dims,
            pixel_format: PixelFormat::Rgba8,
            color_space: ColorSpace::Srgb,
            pixels,
            has_alpha: true,
        }
    }

    fn roundtrip(target: ImageFormat, quality: u8) {
        let decoder = ForgeImageDecoder;
        let encoder = ForgeImageEncoder;
        let opts = ConversionOptions {
            quality,
            ..Default::default()
        };
        let bytes = encoder.encode(&rgba_fixture(), target, &opts).unwrap();
        let back = decoder.decode(&bytes, Some(target)).expect("must redecode");
        assert_eq!(back.dimensions, rgba_fixture().dimensions);
        if target.descriptor().supports_alpha {
            assert!(back.has_alpha || back.pixel_format.has_alpha());
        }
    }

    #[test]
    fn test_png_roundtrip() {
        roundtrip(ImageFormat::Png, 80);
    }

    #[test]
    fn test_jpeg_roundtrip_flattens_alpha() {
        let encoder = ForgeImageEncoder;
        let bytes = encoder
            .encode(
                &rgba_fixture(),
                ImageFormat::Jpeg,
                &ConversionOptions::default(),
            )
            .unwrap();
        // JPEG magic: FF D8 FF.
        assert_eq!(&bytes[0..3], &[0xFF, 0xD8, 0xFF]);
        let back = ForgeImageDecoder
            .decode(&bytes, Some(ImageFormat::Jpeg))
            .unwrap();
        assert!(!back.pixel_format.has_alpha());
    }

    #[test]
    fn test_webp_lossy_roundtrip_quality_80() {
        roundtrip(ImageFormat::Webp, 80);
    }

    #[test]
    fn test_bmp_tiff_roundtrips() {
        roundtrip(ImageFormat::Bmp, 80);
        roundtrip(ImageFormat::Tiff, 80);
    }

    #[test]
    fn test_decode_rejects_empty_and_unknown() {
        let decoder = ForgeImageDecoder;
        assert!(matches!(
            decoder.decode(&[], None),
            Err(ForgeError::InvalidFile(_))
        ));
        assert!(matches!(
            decoder.decode(b"definitely-not-an-image", None),
            Err(ForgeError::UnsupportedFormat(_))
        ));
    }

    #[test]
    fn test_fit_within_step_resizes() {
        let step = FitWithinStep;
        let image = rgba_fixture();
        let opts = ConversionOptions {
            max_dimensions: Some(ImageDimensions::new(2, 2).unwrap()),
            ..Default::default()
        };
        let resized = step.apply(image, &opts).unwrap();
        assert!(resized.dimensions.width <= 2 && resized.dimensions.height <= 2);
        // No-op when unset.
        let image = rgba_fixture();
        let same = step
            .apply(image.clone(), &ConversionOptions::default())
            .unwrap();
        assert_eq!(same.dimensions, image.dimensions);
    }
}
