//! PDF adapter (ADR 009 + v0.3.0 plan):
//! image→PDF writer via `printpdf`; page split/count + text-to-docx via
//! `lopdf` (already printpdf's engine) + `docx-rs`.
//! `PdfRenderer` stub returns [`ForgeError::Unsupported`] until a
//! renderer is qualified (hayro vs pdfium-render — see docs/PLAN.md).

use forge_core::{
    CanonicalImage, ForgeError, PageFit, PageRange, PdfRenderer, PdfSplitter, PdfToDocx,
    PdfWriteSpec, PdfWriter, PixelFormat, Result,
};

/// Max decompressed bytes per page for text extraction (bomb guard;
/// `MemoryLimitExceeded` maps to `ResourceLimitExceeded`, never OOM-loops).
const MAX_PAGE_TEXT_BYTES: usize = 16 * 1024 * 1024;

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

/// PDF page ops over `lopdf` (ADR 009 foresaw direct use for split).
///
/// `delete_pages` numbers refer to ORIGINAL 1-based numbering (verified in
/// `lopdf-0.44/src/processor.rs`): compute the complement set first, delete
/// once, then `prune_objects` the orphans.
#[derive(Debug, Default)]
pub struct LopdfSplitter;

impl PdfSplitter for LopdfSplitter {
    fn page_count(&self, pdf_bytes: &[u8]) -> Result<u32> {
        let doc = lopdf::Document::load_mem(pdf_bytes)
            .map_err(|e| ForgeError::PdfReadFailed(e.to_string()))?;
        Ok(doc.get_pages().len() as u32)
    }

    fn split(&self, pdf_bytes: &[u8], range: &PageRange) -> Result<Vec<u8>> {
        let mut doc = lopdf::Document::load_mem(pdf_bytes)
            .map_err(|e| ForgeError::PdfReadFailed(e.to_string()))?;
        let total = doc.get_pages().len() as u32;
        if total == 0 {
            return Err(ForgeError::InvalidFile("PDF has no pages".to_string()));
        }
        for page in &range.pages {
            if *page == 0 || *page > total {
                return Err(ForgeError::InvalidConfiguration(format!(
                    "page {page} out of range (PDF has {total} pages)"
                )));
            }
        }
        // Delete everything NOT requested (complement of the keep set).
        let keep: std::collections::HashSet<u32> = range.pages.iter().copied().collect();
        let drop: Vec<u32> = (1..=total).filter(|p| !keep.contains(p)).collect();
        doc.delete_pages(&drop);
        doc.prune_objects();
        let mut out = Vec::new();
        doc.save_to(&mut out)
            .map_err(|e| ForgeError::PdfWriteFailed(e.to_string()))?;
        Ok(out)
    }
}

/// PDF → .docx: `lopdf` text extraction + `docx-rs` writer (v0.3.0 plan).
///
/// Text-only export (no images, no layout fidelity): one paragraph per
/// non-empty line, page breaks between pages. Scanned PDFs (no text layer)
/// get a `[No extractable text on page N]` marker — valid .docx, never
/// an empty file, never an error.
#[derive(Debug, Default)]
pub struct LopdfToDocx;

impl PdfToDocx for LopdfToDocx {
    fn convert(&self, pdf_bytes: &[u8], range: Option<&PageRange>) -> Result<Vec<u8>> {
        let doc = lopdf::Document::load_mem(pdf_bytes)
            .map_err(|e| ForgeError::PdfReadFailed(e.to_string()))?;
        let total = doc.get_pages().len() as u32;
        if total == 0 {
            return Err(ForgeError::InvalidFile("PDF has no pages".to_string()));
        }
        let pages: Vec<u32> = match range {
            Some(r) => {
                for page in &r.pages {
                    if *page == 0 || *page > total {
                        return Err(ForgeError::InvalidConfiguration(format!(
                            "page {page} out of range (PDF has {total} pages)"
                        )));
                    }
                }
                r.pages.clone()
            }
            None => (1..=total).collect(),
        };
        let mut word = docx_rs::Docx::new();
        let mut first_page = true;
        for page in pages {
            let text = doc
                .extract_text_with_limit(&[page], MAX_PAGE_TEXT_BYTES)
                .map_err(map_lopdf_extract_error)?;
            let mut wrote_line = false;
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let mut para =
                    docx_rs::Paragraph::new().add_run(docx_rs::Run::new().add_text(line));
                if !first_page && !wrote_line {
                    para = para.page_break_before(true);
                }
                word = word.add_paragraph(para);
                wrote_line = true;
            }
            if !wrote_line {
                let mut para =
                    docx_rs::Paragraph::new().add_run(docx_rs::Run::new().add_text(format!(
                    "[No extractable text on page {page} — scanned image or vector-only content]"
                )));
                if !first_page {
                    para = para.page_break_before(true);
                }
                word = word.add_paragraph(para);
            }
            first_page = false;
        }
        let mut cursor = std::io::Cursor::new(Vec::new());
        word.build()
            .pack(&mut cursor)
            .map_err(|e| ForgeError::EncodeFailed(format!("docx pack: {e}")))?;
        Ok(cursor.into_inner())
    }
}

/// `lopdf` extraction errors → structured variants (bomb-limit is a
/// resource error, not a read error).
fn map_lopdf_extract_error(error: lopdf::Error) -> ForgeError {
    let message = error.to_string();
    if message.contains("MemoryLimitExceeded") {
        ForgeError::ResourceLimitExceeded(format!(
            "PDF page content exceeds {MAX_PAGE_TEXT_BYTES} bytes decompressed"
        ))
    } else {
        ForgeError::PdfReadFailed(message)
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

    /// Multi-page TEXT pdf via printpdf (extractable text, no binary fixture).
    /// One ShowText line per page so lopdf extraction finds real content.
    fn text_pdf(pages: &[&str]) -> Vec<u8> {
        use printpdf::{
            BuiltinFont, Mm, Op, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Pt,
            TextItem,
        };
        let mut doc = PdfDocument::new("ForgeConvert test");
        let mut doc_pages = Vec::with_capacity(pages.len());
        for text in pages {
            doc_pages.push(PdfPage::new(
                Mm(210.0),
                Mm(297.0),
                vec![
                    Op::StartTextSection,
                    Op::SetTextCursor {
                        pos: Point::new(Mm(20.0), Mm(270.0)),
                    },
                    Op::SetFont {
                        font: PdfFontHandle::Builtin(BuiltinFont::Helvetica),
                        size: Pt(14.0),
                    },
                    Op::ShowText {
                        items: vec![TextItem::Text((*text).to_string())],
                    },
                    Op::EndTextSection,
                ],
            ));
        }
        doc.with_pages(doc_pages)
            .save(&PdfSaveOptions::default(), &mut Vec::new())
    }

    fn three_page_pdf() -> Vec<u8> {
        text_pdf(&["alpha page one", "beta page two", "gamma page three"])
    }

    #[test]
    fn test_page_count_three() {
        let splitter = LopdfSplitter;
        assert_eq!(splitter.page_count(&three_page_pdf()).unwrap(), 3);
        assert!(splitter.page_count(b"not a pdf").is_err());
    }

    #[test]
    fn test_split_keeps_selected_pages() {
        let splitter = LopdfSplitter;
        let src = three_page_pdf();
        let out = splitter
            .split(&src, &PageRange::parse("2-3").unwrap())
            .unwrap();
        assert_eq!(&out[0..4], b"%PDF");
        assert_eq!(splitter.page_count(&out).unwrap(), 2);
        // Non-contiguous selection keeps both pages.
        let out = splitter
            .split(&src, &PageRange::parse("1,3").unwrap())
            .unwrap();
        assert_eq!(splitter.page_count(&out).unwrap(), 2);
    }

    #[test]
    fn test_split_rejects_out_of_range_and_garbage() {
        let splitter = LopdfSplitter;
        let src = three_page_pdf();
        let err = splitter
            .split(&src, &PageRange::parse("2-9").unwrap())
            .unwrap_err();
        assert!(matches!(err, ForgeError::InvalidConfiguration(_)));
        let err = splitter
            .split(b"%PDF-1.7 garbage", &PageRange::parse("1").unwrap())
            .unwrap_err();
        assert!(matches!(err, ForgeError::PdfReadFailed(_)));
    }

    #[test]
    fn test_docx_converts_text_with_page_breaks() {
        let conv = LopdfToDocx;
        let out = conv.convert(&three_page_pdf(), None).unwrap();
        // OOXML is a zip: PK magic.
        assert_eq!(&out[0..2], b"PK");
        assert!(out.len() > 500);
        // Ranged export works too.
        let out = conv
            .convert(&three_page_pdf(), Some(&PageRange::parse("1-2").unwrap()))
            .unwrap();
        assert_eq!(&out[0..2], b"PK");
    }

    #[test]
    fn test_docx_rejects_bad_pages_and_bytes() {
        let conv = LopdfToDocx;
        let src = three_page_pdf();
        let err = conv
            .convert(&src, Some(&PageRange::parse("1-99").unwrap()))
            .unwrap_err();
        assert!(matches!(err, ForgeError::InvalidConfiguration(_)));
        let err = conv.convert(b"nope", None).unwrap_err();
        assert!(matches!(err, ForgeError::PdfReadFailed(_)));
    }
}
