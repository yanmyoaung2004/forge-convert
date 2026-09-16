//! PDF adapter (ADR 009): image→PDF writer via `printpdf`;
//! `PdfRenderer` stub returns [`ForgeError::Unsupported`] until a
//! renderer is qualified (hayro vs pdfium-render — see docs/PLAN.md).

use forge_core::{
    CanonicalImage, ForgeError, PageFit, PdfRenderer, PdfWriteSpec, PdfWriter, PixelFormat, Result,
};

/// Image → PDF writer (one page per image, caller-computed layout).
#[derive(Debug, Default)]
pub struct ForgePdfWriter;

impl PdfWriter for ForgePdfWriter {
    fn write_images(&self, images: &[CanonicalImage], spec: &PdfWriteSpec) -> Result<Vec<u8>> {
        if images.is_empty() {
            return Err(ForgeError::InvalidConfiguration(
                "PDF needs at least one image".to_string(),
            ));
        }
        validate_spec(spec)?;

        let mut doc = printpdf::PdfDocument::new("ForgeConvert");
        let mut pages = Vec::with_capacity(images.len());
        for image in images {
            let raw = to_raw_image(image)?;
            let image_id = doc.add_image(&raw);
            let (page_w, page_h) = oriented_page(spec);
            let transform = place_transform(image, spec, &page_w, &page_h);
            pages.push(printpdf::PdfPage::new(
                page_w,
                page_h,
                vec![printpdf::Op::UseXobject {
                    id: image_id.clone(),
                    transform,
                }],
            ));
        }
        Ok(doc
            .with_pages(pages)
            .save(&printpdf::PdfSaveOptions::default(), &mut Vec::new()))
    }
}

/// PDF → image stub: honest `Unsupported` until a renderer qualifies.
#[derive(Debug, Default)]
pub struct StubPdfRenderer;

impl PdfRenderer for StubPdfRenderer {
    fn render(&self, _pdf_bytes: &[u8], _pages: &[u32], _dpi: u16) -> Result<Vec<CanonicalImage>> {
        Err(ForgeError::Unsupported {
            capability: "pdf-to-image",
            hint: "no renderer qualified yet (see ADR 009): rebuild with a render backend",
        })
    }
}

/// Spec guardrails: positive DPI/margins, margins leave drawable area.
fn validate_spec(spec: &PdfWriteSpec) -> Result<()> {
    if !(spec.dpi > 0.0 && spec.dpi <= 1200.0) {
        return Err(ForgeError::InvalidConfiguration(format!(
            "dpi must be 1-1200, got {}",
            spec.dpi
        )));
    }
    if !(spec.margins_mm >= 0.0 && spec.margins_mm < 100.0) {
        return Err(ForgeError::InvalidConfiguration(format!(
            "margins must be 0-100mm, got {}",
            spec.margins_mm
        )));
    }
    let (page_w, page_h) = oriented_page(spec);
    let usable_w = mm_to_pt(page_w) - 2.0 * mm_to_pt(printpdf::Mm(spec.margins_mm as f32));
    let usable_h = mm_to_pt(page_h) - 2.0 * mm_to_pt(printpdf::Mm(spec.margins_mm as f32));
    if usable_w <= 0.0 || usable_h <= 0.0 {
        return Err(ForgeError::InvalidConfiguration(
            "margins leave no drawable area".to_string(),
        ));
    }
    Ok(())
}

/// Page size with orientation applied.
fn oriented_page(spec: &PdfWriteSpec) -> (printpdf::Mm, printpdf::Mm) {
    use forge_core::{Orientation, PageSizeMm};
    let PageSizeMm {
        width_mm,
        height_mm,
    } = spec.page;
    match spec.orientation {
        Orientation::Portrait => (
            printpdf::Mm(width_mm as f32),
            printpdf::Mm(height_mm as f32),
        ),
        Orientation::Landscape => (
            printpdf::Mm(height_mm as f32),
            printpdf::Mm(width_mm as f32),
        ),
    }
}

/// Millimeters → typographic points (printpdf `Mm` handles it, but layout
/// math below works in points).
fn mm_to_pt(mm: printpdf::Mm) -> f64 {
    f64::from(mm.0) * 72.0 / 25.4
}

/// Compute scale + centering for `Fit`/`Fill`/`None` at `spec.dpi`.
fn place_transform(
    image: &CanonicalImage,
    spec: &PdfWriteSpec,
    page_w: &printpdf::Mm,
    page_h: &printpdf::Mm,
) -> printpdf::XObjectTransform {
    let usable_w_pt = mm_to_pt(*page_w) - 2.0 * mm_to_pt(printpdf::Mm(spec.margins_mm as f32));
    let usable_h_pt = mm_to_pt(*page_h) - 2.0 * mm_to_pt(printpdf::Mm(spec.margins_mm as f32));
    // Image physical size at the requested DPI.
    let img_w_pt = f64::from(image.dimensions.width) * 72.0 / spec.dpi;
    let img_h_pt = f64::from(image.dimensions.height) * 72.0 / spec.dpi;
    let (scale_x, scale_y) = match spec.fit {
        PageFit::Fit => {
            let s = (usable_w_pt / img_w_pt).min(usable_h_pt / img_h_pt);
            (s, s)
        }
        PageFit::Fill => {
            let s = (usable_w_pt / img_w_pt).max(usable_h_pt / img_h_pt);
            (s, s)
        }
        PageFit::None => (1.0, 1.0),
    };
    // Center the (scaled) image in the usable rect; printpdf origin is
    // bottom-left, translate from the margin corner.
    let drawn_w = img_w_pt * scale_x;
    let drawn_h = img_h_pt * scale_y;
    let margin_pt = mm_to_pt(printpdf::Mm(spec.margins_mm as f32)) as f32;
    // Note: printpdf places the xobject by its transform; translate puts
    // the image corner at margin + centering offset.
    printpdf::XObjectTransform {
        translate_x: Some(printpdf::Pt(
            margin_pt + ((usable_w_pt - drawn_w).max(0.0) / 2.0) as f32,
        )),
        translate_y: Some(printpdf::Pt(
            margin_pt + ((usable_h_pt - drawn_h).max(0.0) / 2.0) as f32,
        )),
        scale_x: Some(scale_x as f32),
        scale_y: Some(scale_y as f32),
        dpi: Some(spec.dpi as f32),
        ..Default::default()
    }
}

/// Canonical → printpdf `RawImage` (RGBA8/RGB8 only; luma expanded).
fn to_raw_image(image: &CanonicalImage) -> Result<printpdf::RawImage> {
    use printpdf::{RawImage, RawImageData, RawImageFormat};
    let (w, h) = (
        image.dimensions.width as usize,
        image.dimensions.height as usize,
    );
    let expected = CanonicalImage::expected_len(image.dimensions, image.pixel_format);
    if image.pixels.len() != expected {
        return Err(ForgeError::InvalidFile(format!(
            "pixel buffer mismatch: got {}, want {expected}",
            image.pixels.len()
        )));
    }
    let (pixels, data_format) = match image.pixel_format {
        PixelFormat::Rgba8 => (image.pixels.clone(), RawImageFormat::RGBA8),
        PixelFormat::Rgb8 => (image.pixels.clone(), RawImageFormat::RGB8),
        PixelFormat::Luma8 => {
            let mut rgb = Vec::with_capacity(w * h * 3);
            for &l in &image.pixels {
                rgb.extend_from_slice(&[l, l, l]);
            }
            (rgb, RawImageFormat::RGB8)
        }
        PixelFormat::La8 => {
            let mut rgba = Vec::with_capacity(w * h * 4);
            let (pairs, _) = image.pixels.as_chunks::<2>();
            for pair in pairs {
                rgba.extend_from_slice(&[pair[0], pair[0], pair[0], pair[1]]);
            }
            (rgba, RawImageFormat::RGBA8)
        }
    };
    Ok(RawImage {
        pixels: RawImageData::U8(pixels),
        width: w,
        height: h,
        data_format,
        tag: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_core::{ImageDimensions, ImageFormat, PageSizeMm};
    fn rgba_image(width: u32, height: u32) -> CanonicalImage {
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for _ in 0..width as usize * height as usize {
            pixels.extend_from_slice(&[200, 100, 50, 255]);
        }
        CanonicalImage {
            dimensions: ImageDimensions::new(width, height).unwrap(),
            pixel_format: PixelFormat::Rgba8,
            color_space: forge_core::ColorSpace::Srgb,
            pixels,
            has_alpha: true,
        }
    }

    #[test]
    fn test_write_single_image_produces_pdf() {
        let writer = ForgePdfWriter;
        let bytes = writer
            .write_images(&[rgba_image(8, 6)], &PdfWriteSpec::default())
            .unwrap();
        // PDF magic: %PDF.
        assert_eq!(&bytes[0..4], b"%PDF");
        assert!(bytes.len() > 100);
    }

    #[test]
    fn test_write_multi_image_letter_landscape() {
        use forge_core::Orientation;
        let writer = ForgePdfWriter;
        let spec = PdfWriteSpec {
            page: PageSizeMm::LETTER,
            orientation: Orientation::Landscape,
            ..Default::default()
        };
        let bytes = writer
            .write_images(&[rgba_image(4, 4), rgba_image(6, 2)], &spec)
            .unwrap();
        assert_eq!(&bytes[0..4], b"%PDF");
    }

    #[test]
    fn test_write_rejects_empty_and_bad_spec() {
        let writer = ForgePdfWriter;
        assert!(writer.write_images(&[], &PdfWriteSpec::default()).is_err());
        let bad = PdfWriteSpec {
            dpi: 0.0,
            ..Default::default()
        };
        assert!(writer.write_images(&[rgba_image(2, 2)], &bad).is_err());
        let bad_margins = PdfWriteSpec {
            margins_mm: 200.0,
            ..Default::default()
        };
        assert!(writer
            .write_images(&[rgba_image(2, 2)], &bad_margins)
            .is_err());
    }

    #[test]
    fn test_render_stub_returns_unsupported() {
        let renderer = StubPdfRenderer;
        let err = renderer.render(b"%PDF-1.7", &[1], 200).unwrap_err();
        assert!(matches!(err, ForgeError::Unsupported { .. }));
        // And the PDF image target is known-undecodable by the image crate.
        assert!(!ImageFormat::Pdf.descriptor().can_decode);
    }
}
