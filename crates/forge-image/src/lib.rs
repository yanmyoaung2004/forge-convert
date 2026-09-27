//! Image adapter: decode → canonical → transform → encode.
//!
//! - Decode PNG/JPEG/BMP/TIFF/WebP via `image`.
//! - Encode PNG/JPEG/BMP/TIFF via `image`; lossy WebP via `webp`
//!   (ADR 012: `image-webp` is VP8L lossless-only).
//! - RGBA → alpha-less targets flatten over [`BackgroundPolicy`] first.

use forge_core::{
    BackgroundPolicy, CanonicalImage, ColorSpace, Compression, ConversionOptions, ImageDimensions,
    ImageFormat, PixelFormat, ResizeFilter, ResizeSpec, Result,
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
        // EXIF auto-rotate (JPEG/TIFF camera output): apply the orientation
        // transform during decode so downstream sees upright pixels.
        // Malformed EXIF → ignore + proceed (never fail the conversion).
        let dyn_image = apply_exif_orientation(dyn_image, bytes);
        Ok(canonicalize(&dyn_image))
    }
}

/// EXIF orientation → upright pixels (values 1–8 per JEITA CP-3451).
/// Returns the input unchanged for orientation 1 / missing / malformed EXIF.
fn apply_exif_orientation(image: image::DynamicImage, bytes: &[u8]) -> image::DynamicImage {
    let orientation = exif_orientation(bytes);
    match orientation {
        // `image` ops consume/return owned images; geometry per EXIF spec:
        // 2 = flip-H, 3 = 180°, 4 = flip-V, 5 = transpose, 6 = 90°CW, 7 = transverse, 8 = 270°CW.
        None | Some(1) => image,
        Some(2) => image.fliph(),
        Some(3) => image.rotate180(),
        Some(4) => image.flipv(),
        Some(5) => image.rotate90().fliph(),
        Some(6) => image.rotate90(),
        Some(7) => image.rotate270().fliph(),
        Some(8) => image.rotate270(),
        _ => image,
    }
}

/// Read EXIF orientation (1–8) from `bytes`; `None` when absent/unreadable.
/// Pure-Rust `kamadak-exif`, `read_from_container` over an in-memory cursor.
fn exif_orientation(bytes: &[u8]) -> Option<u32> {
    use exif::{In, Reader, Tag};
    let mut cursor = std::io::Cursor::new(bytes);
    let exif = Reader::new().read_from_container(&mut cursor).ok()?;
    let field = exif.get_field(Tag::Orientation, In::PRIMARY)?;
    match field.value.get_uint(0) {
        Some(v @ 1..=8) => Some(v),
        _ => None,
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

    /// Encode. Metadata honesty: re-encoding through `image`/`webp`
    /// encoders does NOT carry EXIF/ICC/XMP chunks — output metadata is
    /// effectively stripped regardless of `MetadataPolicy`. `Preserve`
    /// therefore means "don't strip anything we control" (pixel-affecting
    /// behavior like orientation is already applied at decode); full
    /// metadata round-tripping is future work, tracked, not claimed.
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
            ImageFormat::Webp => match options.compression {
                Some(Compression::WebpLossless) => encode_webp_lossless(image),
                Some(Compression::WebpLossy { quality }) => encode_webp_lossy(image, quality),
                _ => encode_webp_lossy(image, options.quality),
            },
            ImageFormat::Png => match options.compression {
                Some(Compression::Png { level }) => encode_png_level(image, level),
                _ => encode_png(image),
            },
            ImageFormat::Jpeg => match options.compression {
                Some(Compression::Jpeg { quality }) => encode_jpeg(image, quality),
                _ => encode_jpeg(image, options.quality),
            },
            ImageFormat::Bmp | ImageFormat::Tiff => encode_via_image(image, target),
            ImageFormat::Pdf => Err(ForgeError::UnsupportedFormat(
                "PDF is written by forge-pdf, not the image encoder".to_string(),
            )),
            ImageFormat::Docx => Err(ForgeError::UnsupportedFormat(
                "Word export is PDF-only (use pdf-to-docx), not the image encoder".to_string(),
            )),
        }
    }
}
/// Resize step: explicit `options.resize` wins; legacy
/// `options.max_dimensions` falls back to fit-inside (never upscale).
/// `FitWithinStep` stays as a deprecated alias (same behavior).
#[derive(Debug, Default)]
pub struct ResizeStep;

impl TransformStep for ResizeStep {
    fn name(&self) -> &'static str {
        "resize"
    }

    fn apply(&self, image: CanonicalImage, options: &ConversionOptions) -> Result<CanonicalImage> {
        if let Some(spec) = options.resize {
            return apply_resize_spec(image, spec);
        }
        let Some(max) = options.max_dimensions else {
            return Ok(image);
        };
        let target = image.dimensions.fit_within(Some(max));
        if target == image.dimensions {
            return Ok(image);
        }
        resize_canonical(&image, target, ResizeFilter::Lanczos3)
    }
}

/// Legacy name: identical behavior (fit-inside via `max_dimensions`).
pub type FitWithinStep = ResizeStep;

/// Convert `ImageFormat` → `image::ImageFormat` (PDF/Docx have no mapping).
fn to_image_format(format: ImageFormat) -> Option<image::ImageFormat> {
    match format {
        ImageFormat::Png => Some(image::ImageFormat::Png),
        ImageFormat::Jpeg => Some(image::ImageFormat::Jpeg),
        ImageFormat::Webp => Some(image::ImageFormat::WebP),
        ImageFormat::Bmp => Some(image::ImageFormat::Bmp),
        ImageFormat::Tiff => Some(image::ImageFormat::Tiff),
        ImageFormat::Pdf | ImageFormat::Docx => None,
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

/// Resize to `target` with the requested filter (exact dims, aspect
/// decisions made by the caller).
fn resize_canonical(
    image: &CanonicalImage,
    target: ImageDimensions,
    filter: ResizeFilter,
) -> Result<CanonicalImage> {
    let dyn_image = to_dynamic(image)?;
    let resized = dyn_image.resize_exact(target.width, target.height, to_filter(filter));
    let rgba = resized.to_rgba8();
    Ok(CanonicalImage {
        dimensions: target,
        pixel_format: PixelFormat::Rgba8,
        color_space: image.color_space,
        pixels: rgba.into_raw(),
        has_alpha: true,
    })
}

/// Map domain filter → `image` filter.
fn to_filter(filter: ResizeFilter) -> image::imageops::FilterType {
    match filter {
        ResizeFilter::Lanczos3 => image::imageops::FilterType::Lanczos3,
        ResizeFilter::CatmullRom => image::imageops::FilterType::CatmullRom,
        ResizeFilter::Gaussian => image::imageops::FilterType::Gaussian,
        ResizeFilter::Nearest => image::imageops::FilterType::Nearest,
    }
}

/// Apply an explicit [`ResizeSpec`]: exact / fit (honors `upscale`) /
/// fill (cover + center-crop). No-op fast paths avoid resampling.
fn apply_resize_spec(image: CanonicalImage, spec: ResizeSpec) -> Result<CanonicalImage> {
    match spec {
        ResizeSpec::Exact { dimensions, filter } => {
            if dimensions == image.dimensions {
                return Ok(image);
            }
            resize_canonical(&image, dimensions, filter)
        }
        ResizeSpec::Fit {
            bounds,
            filter,
            upscale,
        } => {
            if !upscale
                && image.dimensions.width <= bounds.width
                && image.dimensions.height <= bounds.height
            {
                return Ok(image);
            }
            let target = image.dimensions.scaled_to_fit(bounds);
            if target == image.dimensions {
                return Ok(image);
            }
            resize_canonical(&image, target, filter)
        }
        ResizeSpec::Fill {
            bounds,
            filter,
            upscale,
        } => {
            if !upscale
                && image.dimensions.width <= bounds.width
                && image.dimensions.height <= bounds.height
            {
                // Still need exact bounds: upscale required → honor `upscale: false`
                // by returning the source (never enlarge silently).
                return Ok(image);
            }
            let covered = image.dimensions.scaled_to_cover(bounds);
            let resized = resize_canonical(&image, covered, filter)?;
            center_crop(&resized, bounds)
        }
    }
}

/// Center-crop `image` to exactly `target` (clamped, never panics).
fn center_crop(image: &CanonicalImage, target: ImageDimensions) -> Result<CanonicalImage> {
    if image.dimensions == target {
        return Ok(image.clone());
    }
    let dyn_image = to_dynamic(image)?;
    let rgba = dyn_image.to_rgba8();
    let (ox, oy) = image.dimensions.center_crop_origin(target);
    // Clamp crop box inside the buffer.
    let ox = ox.min(image.dimensions.width.saturating_sub(target.width));
    let oy = oy.min(image.dimensions.height.saturating_sub(target.height));
    let mut pixels = Vec::with_capacity(target.width as usize * target.height as usize * 4);
    for row in 0..target.height {
        let src_y = oy + row;
        for col in 0..target.width {
            let src_x = ox + col;
            let pixel = rgba.get_pixel(src_x, src_y);
            pixels.extend_from_slice(&pixel.0);
        }
    }
    Ok(CanonicalImage {
        dimensions: target,
        pixel_format: PixelFormat::Rgba8,
        color_space: image.color_space,
        pixels,
        has_alpha: true,
    })
}

/// PNG via `image` (default compression; `oxipng` later in Phase 8).
/// Preserves alpha: RGBA/La in → RGBA out, others → RGB out.
fn encode_png(image: &CanonicalImage) -> Result<Vec<u8>> {
    encode_png_level(image, 6)
}

/// PNG with explicit level 0–9: 0–2 → Fast, 3–6 → Default, 7–9 → Best.
fn encode_png_level(image: &CanonicalImage, level: u8) -> Result<Vec<u8>> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::{ColorType, ExtendedColorType};
    let compression = match level {
        0..=2 => CompressionType::Fast,
        3..=6 => CompressionType::Default,
        _ => CompressionType::Best,
    };
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
        let encoder = PngEncoder::new_with_quality(&mut out, compression, FilterType::Adaptive);
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

/// Lossless WebP via libwebp `encode_lossless` (exact pixels, bigger files).
fn encode_webp_lossless(image: &CanonicalImage) -> Result<Vec<u8>> {
    let dyn_image = to_dynamic(image)?;
    let rgba = dyn_image.to_rgba8();
    let encoder = webp::Encoder::from_rgba(
        rgba.as_raw(),
        image.dimensions.width,
        image.dimensions.height,
    );
    Ok(encoder.encode_lossless().to_vec())
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
        let step = ResizeStep;
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

    #[test]
    fn test_resize_exact_1020x900_to_800x600() {
        // The user's example: explicit exact resize wins over aspect.
        let step = ResizeStep;
        let image = CanonicalImage {
            dimensions: ImageDimensions::new(1020, 900).unwrap(),
            pixel_format: PixelFormat::Rgba8,
            color_space: ColorSpace::Srgb,
            pixels: [128, 64, 32, 255].repeat(1020 * 900),
            has_alpha: true,
        };
        let opts = ConversionOptions {
            resize: Some(ResizeSpec::Exact {
                dimensions: ImageDimensions::new(800, 600).unwrap(),
                filter: ResizeFilter::Lanczos3,
            }),
            ..Default::default()
        };
        let out = step.apply(image, &opts).unwrap();
        assert_eq!(out.dimensions, ImageDimensions::new(800, 600).unwrap());
        assert_eq!(out.pixels.len(), 800 * 600 * 4);
    }

    #[test]
    fn test_resize_fit_no_upscale_and_fill_crops() {
        let step = ResizeStep;
        let small = CanonicalImage {
            dimensions: ImageDimensions::new(4, 3).unwrap(),
            pixel_format: PixelFormat::Rgba8,
            color_space: ColorSpace::Srgb,
            pixels: [9, 9, 9, 255].repeat(4 * 3),
            has_alpha: true,
        };
        let opts = ConversionOptions {
            resize: Some(ResizeSpec::Fit {
                bounds: ImageDimensions::new(800, 600).unwrap(),
                filter: ResizeFilter::CatmullRom,
                upscale: false,
            }),
            ..Default::default()
        };
        assert_eq!(
            step.apply(small.clone(), &opts).unwrap().dimensions,
            small.dimensions
        );
        // Fill 3×2 → cover+crop to exactly 3×2.
        let opts = ConversionOptions {
            resize: Some(ResizeSpec::Fill {
                bounds: ImageDimensions::new(3, 2).unwrap(),
                filter: ResizeFilter::Nearest,
                upscale: true,
            }),
            ..Default::default()
        };
        let out = step.apply(rgba_fixture(), &opts).unwrap();
        assert_eq!(out.dimensions, ImageDimensions::new(3, 2).unwrap());
    }

    #[test]
    fn test_png_levels_order_and_webp_lossless_decodes() {
        let encoder = ForgeImageEncoder;
        let opts_for = |compression: Compression| ConversionOptions {
            compression: Some(compression),
            ..Default::default()
        };
        let fast = encoder
            .encode(
                &rgba_fixture(),
                ImageFormat::Png,
                &opts_for(Compression::Png { level: 0 }),
            )
            .unwrap();
        let best = encoder
            .encode(
                &rgba_fixture(),
                ImageFormat::Png,
                &opts_for(Compression::Png { level: 9 }),
            )
            .unwrap();
        // Same pixels, both valid PNG; best compresses at least as well.
        assert_eq!(
            &fast[0..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        );
        assert!(
            best.len() <= fast.len(),
            "level 9 ≤ level 0 ({} vs {})",
            best.len(),
            fast.len()
        );
        let lossless = encoder
            .encode(
                &rgba_fixture(),
                ImageFormat::Webp,
                &opts_for(Compression::WebpLossless),
            )
            .unwrap();
        let back = ForgeImageDecoder
            .decode(&lossless, Some(ImageFormat::Webp))
            .unwrap();
        assert_eq!(back.dimensions, rgba_fixture().dimensions);
    }

    /// Build a minimal JPEG with an EXIF orientation tag by hand:
    /// SOI + APP1(Exif\0\0 + TIFF LE header + IFD0[Orientation=SHORT `value`]) + EOI.
    /// Decoders tolerate the truncated payload for orientation parsing purposes;
    /// `exif_orientation` only needs the header, not image data.
    fn jpeg_with_orientation(value: u16) -> Vec<u8> {
        let mut exif_payload = b"Exif\0\0".to_vec();
        // TIFF header: II, 42, IFD0 offset 8.
        exif_payload.extend_from_slice(&[0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00]);
        // IFD0: 1 entry.
        exif_payload.extend_from_slice(&[0x01, 0x00]);
        // Tag 0x0112 (Orientation), type SHORT (3), count 1, value + pad.
        exif_payload.extend_from_slice(&[0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00]);
        exif_payload.extend_from_slice(&value.to_le_bytes());
        exif_payload.extend_from_slice(&[0x00, 0x00]);
        // Next IFD offset 0.
        exif_payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1];
        let len = (exif_payload.len() + 2) as u16;
        jpeg.extend_from_slice(&len.to_be_bytes());
        jpeg.extend_from_slice(&exif_payload);
        jpeg.extend_from_slice(&[0xFF, 0xD9]);
        jpeg
    }

    #[test]
    fn test_exif_orientation_parsed_1_to_8() {
        for value in 1..=8u16 {
            assert_eq!(
                exif_orientation(&jpeg_with_orientation(value)),
                Some(value as u32),
                "orientation {value}"
            );
        }
    }

    #[test]
    fn test_exif_orientation_missing_or_malformed_is_none() {
        assert_eq!(exif_orientation(b"not-a-jpeg"), None);
        assert_eq!(exif_orientation(&[0xFF, 0xD8, 0xFF, 0xD9]), None);
        // Truncated EXIF payload → parse fails → None, never panics.
        assert_eq!(
            exif_orientation(&[0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x08, 0x45, 0x78]),
            None
        );
        // Orientation value 0 / 9 out of range → None.
        assert_eq!(exif_orientation(&jpeg_with_orientation(0)), None);
        assert_eq!(exif_orientation(&jpeg_with_orientation(9)), None);
    }

    #[test]
    fn test_apply_orientation_swaps_dims_for_90_270() {
        // 4×2 landscape fixture; orientation 6 (90°CW) → 2×4 portrait.
        let landscape = CanonicalImage {
            dimensions: ImageDimensions::new(4, 2).unwrap(),
            pixel_format: PixelFormat::Rgba8,
            color_space: ColorSpace::Srgb,
            pixels: [10, 20, 30, 255].repeat(4 * 2),
            has_alpha: true,
        };
        let dyn_image = to_dynamic(&landscape).unwrap();
        // Simulate decode-time rotation by calling the transform directly.
        let rotated = apply_exif_orientation(dyn_image, &jpeg_with_orientation(6));
        assert_eq!((rotated.width(), rotated.height()), (2, 4));
        // Orientation 1 / missing → untouched dims.
        let dyn_image = to_dynamic(&landscape).unwrap();
        let same = apply_exif_orientation(dyn_image, &jpeg_with_orientation(1));
        assert_eq!((same.width(), same.height()), (4, 2));
    }
}
