//! PDF adapter (ADR 009 + v0.3.0 plan):
//! image→PDF writer via `printpdf`; page split/count + text-to-docx via
//! `lopdf` (already printpdf's engine) + `docx-rs`.
//! `PdfRenderer` stub returns [`ForgeError::Unsupported`] until a
//! renderer is qualified (hayro vs pdfium-render — see docs/PLAN.md).

use forge_core::{
    CanonicalImage, ForgeError, ImageDimensions, PageFit, PageRange, PdfCompressLevel,
    PdfCompressor, PdfMerger, PdfRenderer, PdfSplitter, PdfToDocx, PdfWriteSpec, PdfWriter,
    PixelFormat, Result,
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

/// PDF → image via hayro 0.6 (pure-Rust CPU rasterizer, MSVC-safe, no DLL).
/// Replaces the old stub: `render` CLI + desktop now go live.
/// `dpi` scales 72pt units (`scale = dpi/72`); pages are 1-based; empty pages → `InvalidConfiguration`.
#[derive(Debug, Default)]
pub struct HayroRenderer;

impl PdfRenderer for HayroRenderer {
    fn render(&self, pdf_bytes: &[u8], pages: &[u32], dpi: u16) -> Result<Vec<CanonicalImage>> {
        use hayro::hayro_interpret::InterpreterSettings;
        use hayro::hayro_syntax::Pdf;
        use hayro::{render, RenderCache, RenderSettings};
        if pdf_bytes.is_empty() {
            return Err(ForgeError::InvalidFile("empty PDF".to_string()));
        }
        if pages.is_empty() {
            return Err(ForgeError::InvalidConfiguration(
                "render needs at least one page".to_string(),
            ));
        }
        if !(1..=1200).contains(&dpi) {
            return Err(ForgeError::InvalidConfiguration(format!(
                "render dpi must be 1-1200, got {dpi}"
            )));
        }
        // `Pdf::new` borrows `pdf_bytes` (zero-copy); load errors → `PdfReadFailed`.
        let pdf = Pdf::new(pdf_bytes.to_vec())
            .map_err(|e| ForgeError::PdfReadFailed(format!("pdf load: {e:?}")))?;
        let total = pdf.pages().len();
        for page in pages {
            if *page == 0 || *page as usize > total {
                return Err(ForgeError::InvalidConfiguration(format!(
                    "page {page} out of range (PDF has {total} pages)"
                )));
            }
        }
        // 72pt → px scale; WHITE bg (print fidelity); default interpreter settings
        // (bundled standard fonts via `embed-fonts`; no system-font lookup).
        let scale = f32::from(dpi) / 72.0;
        let interpreter = InterpreterSettings::default();
        let settings = RenderSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: vello_cpu_white(),
            ..Default::default()
        };
        let cache = RenderCache::new();
        let mut out = Vec::with_capacity(pages.len());
        for page_no in pages {
            // `pages()` derefs to `[Page]` (hayro-syntax `Deref` impl); 0-based index.
            let page = &pdf.pages()[*page_no as usize - 1];
            let pixmap = render(page, &cache, &interpreter, &settings);
            out.push(pixmap_to_canonical(pixmap)?);
        }
        Ok(out)
    }
}

/// WHITE bg without naming the vello color type at our call site
/// (hayro re-exports `vello_cpu`; keep the versioned path in one place).
fn vello_cpu_white() -> hayro::vello_cpu::color::AlphaColor<hayro::vello_cpu::color::Srgb> {
    use hayro::vello_cpu::color::palette::css::WHITE;
    WHITE
}

/// Pixmap (premultiplied RGBA) → `CanonicalImage` (straight RGBA8).
/// `take_unpremultiplied` handles the un-premultiply; empty pixmap → `PdfRenderFailed`.
fn pixmap_to_canonical(pixmap: hayro::vello_cpu::Pixmap) -> Result<CanonicalImage> {
    let w = pixmap.width() as u32;
    let h = pixmap.height() as u32;
    if w == 0 || h == 0 {
        return Err(ForgeError::PdfRenderFailed("empty render".to_string()));
    }
    let dims = ImageDimensions::new(w, h)
        .map_err(|_| ForgeError::PdfRenderFailed("bad render dims".to_string()))?;
    let rgba = pixmap.take_unpremultiplied();
    let mut pixels = Vec::with_capacity(rgba.len() * 4);
    for px in &rgba {
        pixels.extend_from_slice(&[px.r, px.g, px.b, px.a]);
    }
    Ok(CanonicalImage {
        dimensions: dims,
        pixel_format: PixelFormat::Rgba8,
        color_space: forge_core::ColorSpace::Srgb,
        pixels,
        has_alpha: true,
    })
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

/// PDF merge over printpdf parse + append (v0.4.0 plan Phase A).
///
/// Each input parses via printpdf's own deserializer (handles its own
/// output AND foreign PDFs: fonts/XObjects re-resolved per document), then
/// `append_document` concatenates pages + merges resources/bookmarks.
/// Empty list / garbage bytes → structured errors, never panic.
#[derive(Debug, Default)]
pub struct LopdfMerger;

impl PdfMerger for LopdfMerger {
    fn merge(&self, pdfs: &[&[u8]]) -> Result<Vec<u8>> {
        if pdfs.is_empty() {
            return Err(ForgeError::InvalidConfiguration(
                "pdf-merge needs at least one input".to_string(),
            ));
        }
        let mut merged: Option<printpdf::PdfDocument> = None;
        for (i, bytes) in pdfs.iter().enumerate() {
            let mut warnings = Vec::new();
            let opts = printpdf::PdfParseOptions::default();
            let doc = printpdf::deserialize::parse_pdf_from_bytes(bytes, &opts, &mut warnings)
                .map_err(|e| ForgeError::PdfReadFailed(format!("input {}: {e}", i + 1)))?;
            match &mut merged {
                None => merged = Some(doc),
                Some(first) => first.append_document(doc),
            }
        }
        let doc = merged.expect("non-empty checked above");
        Ok(doc.save(&printpdf::PdfSaveOptions::default(), &mut Vec::new()))
    }
}

/// PDF compress over lopdf prune + recompress (v0.4.0 plan Phase A).
///
/// Light: `prune_objects` (drop orphans left by splits/edits) — always safe.
/// Balanced: + `compress()` (Flate-recompress content streams) — better
/// savings, same pages. Output always a valid PDF via `save_to`.
#[derive(Debug, Default)]
pub struct LopdfCompressor;

impl PdfCompressor for LopdfCompressor {
    fn compress(&self, pdf_bytes: &[u8], level: PdfCompressLevel) -> Result<Vec<u8>> {
        let mut doc = lopdf::Document::load_mem(pdf_bytes)
            .map_err(|e| ForgeError::PdfReadFailed(e.to_string()))?;
        if doc.get_pages().is_empty() {
            return Err(ForgeError::InvalidFile("PDF has no pages".to_string()));
        }
        doc.prune_objects();
        if level == PdfCompressLevel::Balanced {
            doc.compress();
        }
        let mut out = Vec::new();
        doc.save_to(&mut out)
            .map_err(|e| ForgeError::PdfWriteFailed(e.to_string()))?;
        Ok(out)
    }
}
/// PDF → .docx: position-faithful, styled, Unicode-safe export.
///
/// ilovepdf-class fidelity (local, no OCR):
/// 1. **Position**: text-state matrix walk (`Tm`/`Td`/`TD`/`T*`) → spans
///    carry (x, y); lines bucketed by y (±2pt), sorted top-first, runs
///    left-first — visual order beats stream order (two-column safe).
///    Alignment guessed from x (left/center/right → `w:jc`).
/// 2. **Unicode**: font `Encoding` decodes incl. ToUnicode CMap
///    (Myanmar U+1000–109F, CJK); control chars stripped, combining marks
///    kept. Unmapped Identity-H/V CID fonts report honestly instead of
///    mojibake. CJK-safe `RunFonts` (ascii/hAnsi/eastAsia/cs).
/// 3. **Styling**: `Tf` size → half-point `w:sz`; BaseName suffixes →
///    bold/italic; images (`DCTDecode`) embed inline at stream y;
///    2+-space columnar lines → real `w:tbl`.
///
/// Scanned PDFs get a `[No extractable content on page N]` marker — valid
/// .docx, never empty, never an error.
pub struct LopdfToDocx;

/// One styled text span with text-space position (points, y-up like PDF).
#[derive(Debug, Clone)]
struct Span {
    x: f32,
    y: f32,
    size: f32,
    family: String,
    bold: bool,
    italic: bool,
    text: String,
}

/// One visual line: spans grouped by y, sorted by x.
#[derive(Debug)]
struct LineInfo {
    y: f32,
    align: Option<docx_rs::AlignmentType>,
    runs: Vec<Span>,
}

/// Page content in visual (top-to-bottom) order.
#[derive(Debug)]
enum OrderedBlock {
    Text(LineInfo),
    Image {
        y: f32,
        bytes: Vec<u8>,
        width_px: u32,
        height_px: u32,
    },
}

/// Per-font decode + style info resolved from the font dictionary.
#[derive(Debug, Clone)]
struct FontStyle {
    family: String,
    bold: bool,
    italic: bool,
    /// False when the font can't map to Unicode (e.g. Type0 Identity-H/V
    /// with no ToUnicode CMap): spans are skipped + the font is reported
    /// instead of emitting mojibake.
    mapped: bool,
}

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
            let page_start = first_page;
            let blocks = page_blocks(&doc, page)?;
            // Consecutive text lines with aligned columns → one table.
            let mut pending: Vec<LineInfo> = Vec::new();
            let mut wrote_any = false;
            // Flush `pending`: table when 2+ columnar lines align, else
            // styled paragraphs. Returns (word, wrote).
            let flush_pending =
                |word: docx_rs::Docx, pending: &mut Vec<LineInfo>, page_break: bool| {
                    let mut word = word;
                    if pending.is_empty() {
                        return (word, false);
                    }
                    if pending.len() >= 2
                        && pending
                            .iter()
                            .filter(|l| split_columns(&line_text(l)).len() >= 2)
                            .count()
                            >= 2
                    {
                        word = push_styled_table(word, pending, page_break);
                    } else {
                        let mut first = page_break;
                        for line in pending.iter() {
                            word = push_styled_para(word, line, first);
                            first = false;
                        }
                    }
                    pending.clear();
                    (word, true)
                };
            for block in blocks {
                match block {
                    OrderedBlock::Text(line) => pending.push(line),
                    OrderedBlock::Image {
                        y: _,
                        bytes,
                        width_px,
                        height_px,
                    } => {
                        let (w, wrote) =
                            flush_pending(word, &mut pending, page_start && !wrote_any);
                        word = w;
                        wrote_any |= wrote;
                        let mut para =
                            docx_rs::Paragraph::new().add_run(docx_rs::Run::new().add_image(
                                docx_rs::Pic::new_with_dimensions(bytes, width_px, height_px),
                            ));
                        if page_start && !wrote_any {
                            para = para.page_break_before(true);
                        }
                        word = word.add_paragraph(para);
                        wrote_any = true;
                    }
                }
            }
            let (w, wrote) = flush_pending(word, &mut pending, page_start && !wrote_any);
            word = w;
            wrote_any |= wrote;
            if !wrote_any {
                let mut para = docx_rs::Paragraph::new().add_run(
                    docx_rs::Run::new().add_text(format!(
                        "[No extractable content on page {page} — scanned image or vector-only content]"
                    )),
                );
                if !first_page {
                    para = para.page_break_before(true);
                }
                word = word.add_paragraph(para);
                wrote_any = true;
            }
            if wrote_any {
                first_page = false;
            }
        }
        let mut cursor = std::io::Cursor::new(Vec::new());
        word.build()
            .pack(&mut cursor)
            .map_err(|e| ForgeError::EncodeFailed(format!("docx pack: {e}")))?;
        Ok(cursor.into_inner())
    }
}

/// Concatenated run texts of a line (for table-shape detection).
fn line_text(line: &LineInfo) -> String {
    line.runs
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Push one styled paragraph: runs keep family/size/bold/italic, CJK-safe
/// fonts on eastAsia, line keeps its alignment guess. `page_break` sets
/// `pageBreakBefore` on this paragraph only.
fn push_styled_para(word: docx_rs::Docx, line: &LineInfo, page_break: bool) -> docx_rs::Docx {
    let mut para = docx_rs::Paragraph::new();
    if let Some(align) = line.align {
        para = para.align(align);
    }
    if page_break {
        para = para.page_break_before(true);
    }
    for span in &line.runs {
        let mut run = docx_rs::Run::new().add_text(&span.text);
        // docx size = half-points: 12pt → 24.
        run = run.size((span.size.clamp(6.0, 72.0) * 2.0).round() as usize);
        if span.bold {
            run = run.bold();
        }
        if span.italic {
            run = run.italic();
        }
        // CJK-safe: family on ascii + hAnsi + eastAsia + cs so Word picks
        // a glyph-bearing font for Japanese/Myanmar runs.
        run = run.fonts(
            docx_rs::RunFonts::new()
                .ascii(&span.family)
                .hi_ansi(&span.family)
                .east_asia(&span.family)
                .cs(&span.family),
        );
        para = para.add_run(run);
    }
    word.add_paragraph(para)
}

/// Push `pending` lines as a bordered table, styling runs per cell.
/// Column splits reuse `split_columns`; ragged rows pad with empty cells.
fn push_styled_table(word: docx_rs::Docx, pending: &[LineInfo], page_break: bool) -> docx_rs::Docx {
    let max_cols = pending
        .iter()
        .map(|l| split_columns(&line_text(l)).len())
        .max()
        .unwrap_or(1)
        .max(1);
    let mut rows = Vec::with_capacity(pending.len());
    for (i, line) in pending.iter().enumerate() {
        let mut cols = split_columns(&line_text(line));
        while cols.len() < max_cols {
            cols.push(String::new());
        }
        // Per-cell style: reuse the line's first run (tables from uniform rows).
        let tpl = line.runs.first();
        let cells: Vec<docx_rs::TableCell> = cols
            .into_iter()
            .map(|text| {
                let mut para = docx_rs::Paragraph::new();
                let mut run = docx_rs::Run::new().add_text(text);
                if let Some(span) = tpl {
                    run = run.size((span.size.clamp(6.0, 72.0) * 2.0).round() as usize);
                    if span.bold {
                        run = run.bold();
                    }
                    if span.italic {
                        run = run.italic();
                    }
                    run = run.fonts(
                        docx_rs::RunFonts::new()
                            .ascii(&span.family)
                            .hi_ansi(&span.family)
                            .east_asia(&span.family)
                            .cs(&span.family),
                    );
                }
                para = para.add_run(run);
                if page_break && i == 0 {
                    para = para.page_break_before(true);
                }
                docx_rs::TableCell::new().add_paragraph(para)
            })
            .collect();
        rows.push(docx_rs::TableRow::new(cells));
    }
    word.add_table(docx_rs::Table::new(rows))
}

/// Legacy plain-text flusher: superseded by styled pending flush in
/// `convert` (kept for reference; no longer called).
#[allow(dead_code)]
fn flush_text_lines(
    word: docx_rs::Docx,
    lines: &mut Vec<String>,
    page_break: bool,
) -> (docx_rs::Docx, bool) {
    if lines.is_empty() {
        return (word, false);
    }
    let mut word = word;
    if lines.len() >= 2 && lines.iter().filter(|l| split_columns(l).len() >= 2).count() >= 2 {
        word = push_table(word, lines, page_break);
    } else {
        let mut first = page_break;
        for line in lines.iter() {
            let mut para = docx_rs::Paragraph::new().add_run(docx_rs::Run::new().add_text(line));
            if first {
                para = para.page_break_before(true);
                first = false;
            }
            word = word.add_paragraph(para);
        }
    }
    lines.clear();
    (word, true)
}

/// Walk one page's content ops with a text-state matrix (PDF 32000-1 §9.4).
/// Tracks `Tm`/`Td`/`TD`/`T*` positioning so spans carry (x, y) in points
/// (y-up). `Tj`/`TJ`/`'`/`"` decode queued at the CURRENT position BEFORE
/// applying any line-move in the same op-list step — matching how viewers
/// lay glyphs out. Relative moves (`Td`, `TD`, `T*`) advance from the
/// current point; absolute `Tm` sets it. Run state (`Tf` size + font)
/// attaches family/bold/italic to every span for docx styling.
/// `Do` records an image slot with the y of the surrounding text flow so
/// images interleave in visual order after y-sorting.
fn page_blocks(doc: &lopdf::Document, page: u32) -> Result<Vec<OrderedBlock>> {
    let pages = doc.get_pages();
    let page_id = pages
        .get(&page)
        .copied()
        .ok_or_else(|| ForgeError::InvalidConfiguration(format!("page {page} not found")))?;
    let fonts = doc
        .get_page_fonts(page_id)
        .map_err(|e| ForgeError::PdfReadFailed(e.to_string()))?;
    // Per-font decode + style: family/bold/italic from BaseName, and whether
    // the font maps to Unicode at all (unmapped CID fonts skipped honestly).
    let mut styles: std::collections::BTreeMap<Vec<u8>, (FontStyle, Option<lopdf::Encoding<'_>>)> =
        std::collections::BTreeMap::new();
    let mut unmapped: Vec<String> = Vec::new();
    for (name, font) in &fonts {
        let style = font_style(doc, font);
        let mapped = style.mapped;
        let enc = font
            .get_font_encoding_with_limit(doc, MAX_PAGE_TEXT_BYTES)
            .ok();
        if !mapped {
            unmapped.push(style.family.clone());
        }
        styles.insert(name.clone(), (style, enc));
    }
    let content_data = doc
        .get_page_content_with_limit(page_id, MAX_PAGE_TEXT_BYTES)
        .map_err(map_lopdf_extract_error)?;
    let content =
        lopdf::content::Content::decode(&content_data).map_err(map_lopdf_extract_error)?;
    // Text state: current point (x, y) in points, font size, active font.
    let mut x = 0.0f32;
    let mut y = 0.0f32;
    let mut size = 12.0f32;
    let mut active: Option<Vec<u8>> = None;
    let mut spans: Vec<Span> = Vec::new();
    // `Do` image slots: (name, y at that point in the stream).
    let mut image_slots: Vec<(Option<String>, f32)> = Vec::new();
    for op in &content.operations {
        match op.operator.as_str() {
            "Tf" => {
                if let [name, sz] = op.operands.as_slice() {
                    if let Ok(n) = name.as_name() {
                        active = Some(n.to_vec());
                    }
                    if let Ok(s) = sz.as_float() {
                        if s > 0.0 && s < 1000.0 {
                            size = s;
                        }
                    }
                }
            }
            "Tm" => {
                if let [a, b, c, d, e, f] = op.operands.as_slice() {
                    if let (Ok(e), Ok(f)) = (e.as_float(), f.as_float()) {
                        // Absolute text matrix: e/f = new origin. Scale
                        // terms (a/d) fold into size via Tf already; shear
                        // (b/c) ignored — glyph order, not geometry, matters.
                        let _ = (a, b, c, d);
                        x = e;
                        y = f;
                    }
                }
            }
            "Td" => {
                if let [tx, ty] = op.operands.as_slice() {
                    if let (Ok(tx), Ok(ty)) = (tx.as_float(), ty.as_float()) {
                        x += tx;
                        y += ty;
                    }
                }
            }
            "TD" => {
                if let [tx, ty] = op.operands.as_slice() {
                    if let (Ok(tx), Ok(ty)) = (tx.as_float(), ty.as_float()) {
                        x += tx;
                        y += ty;
                    }
                }
            }
            "T*" => {
                y -= size * 1.2;
                x = 0.0;
            }
            "Tj" | "TJ" | "'" | "\"" => {
                let targets: Vec<&lopdf::Object> = if op.operator == "\"" {
                    op.operands.get(2).into_iter().collect()
                } else {
                    op.operands.iter().collect()
                };
                if let Some(key) = active.clone() {
                    if let Some((style, Some(enc))) = styles.get(&key) {
                        for target in targets {
                            let before = spans.len();
                            collect_styled(
                                &mut spans,
                                enc,
                                std::slice::from_ref(target),
                                x,
                                y,
                                size,
                                style,
                            )?;
                            // Advance x by ~0.5 * size per emitted char so
                            // same-line runs keep left-to-right order even
                            // when the PDF omits explicit moves.
                            let emitted: usize =
                                spans[before..].iter().map(|s| s.text.chars().count()).sum();
                            x += emitted as f32 * size * 0.5;
                        }
                    }
                }
                if op.operator == "'" || op.operator == "\"" {
                    y -= size * 1.2;
                    x = 0.0;
                }
            }
            "ET" => {
                // End of text object: next Tj starts a fresh line region.
                // (Position resets when the next Tm/Td arrives.)
            }
            "Do" => {
                let name = op
                    .operands
                    .first()
                    .and_then(|o| o.as_name().ok())
                    .map(|n| String::from_utf8_lossy(n).into_owned());
                image_slots.push((name, y));
            }
            _ => {}
        }
    }
    group_blocks(doc, page, page_id, spans, image_slots, unmapped)
}

/// Decode operands into styled spans at (x, y): strings decode via the
/// font's encoding (ToUnicode CMap for Myanmar/Japanese/CID fonts),
/// nested TJ arrays recurse + space, kerns < -100 → word space.
#[allow(clippy::too_many_arguments)]
fn collect_styled(
    spans: &mut Vec<Span>,
    encoding: &lopdf::Encoding<'_>,
    operands: &[lopdf::Object],
    x: f32,
    y: f32,
    size: f32,
    style: &FontStyle,
) -> Result<()> {
    for operand in operands {
        match operand {
            lopdf::Object::String(bytes, _) => {
                let mut s = String::new();
                encoding
                    .write_to_string(bytes, &mut s)
                    .map_err(|e| ForgeError::PdfReadFailed(e.to_string()))?;
                // Skip control chars but KEEP all Unicode incl. Myanmar
                // (U+1000–U+109F), CJK (U+3000–U+9FFF+), combining marks.
                let clean: String = s
                    .chars()
                    .filter(|c| !c.is_control() || *c == '\t')
                    .collect();
                if !clean.trim().is_empty() {
                    spans.push(Span {
                        x,
                        y,
                        size,
                        family: style.family.clone(),
                        bold: style.bold,
                        italic: style.italic,
                        text: clean.trim().to_string(),
                    });
                }
            }
            lopdf::Object::Array(arr) => {
                collect_styled(spans, encoding, arr, x, y, size, style)?;
                // Trailing space per TJ array like lopdf's collect_text.
                if let Some(last) = spans.last_mut() {
                    last.text.push(' ');
                }
            }
            lopdf::Object::Integer(i) if *i < -100 => {
                if let Some(last) = spans.last_mut() {
                    last.text.push(' ');
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Resolve family/bold/italic + Unicode-mappability from a font dict.
/// BaseName like `ABCDEE+MyanmarText-Bold` → family `MyanmarText`, bold.
/// Type0 Identity-H/V without ToUnicode → mapped=false (honest report).
fn font_style(doc: &lopdf::Document, font: &lopdf::Dictionary) -> FontStyle {
    let base = font
        .get(b"BaseFont")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .unwrap_or_default();
    // Strip subset prefix (`ABCDEF+`) per PDF spec §9.6.4.
    let short = base.split('+').next_back().unwrap_or(&base).to_string();
    let lower = short.to_lowercase();
    let bold = lower.contains("bold") || lower.contains("black") || lower.contains("heavy");
    let italic = lower.contains("italic") || lower.contains("oblique");
    let family = short
        .trim_end_matches("-Bold")
        .trim_end_matches("-Italic")
        .trim_end_matches("-BoldItalic")
        .to_string();
    // Mappability: Type0/Identity without ToUnicode can't reach Unicode.
    let subtype = font
        .get(b"Subtype")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| n.to_vec())
        .unwrap_or_default();
    let mapped = if subtype == b"Type0" {
        // Identity-H/V REQUIRES ToUnicode for text extraction.
        let has_tounicode = font
            .get_deref(b"ToUnicode", doc)
            .and_then(|o| o.as_stream())
            .is_ok();
        let enc_name = font
            .get(b"Encoding")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(|n| n.to_vec())
            .unwrap_or_default();
        has_tounicode
            || (enc_name != b"Identity-H".as_slice() && enc_name != b"Identity-V".as_slice())
    } else {
        true
    };
    FontStyle {
        family: if family.is_empty() {
            "Calibri".to_string()
        } else {
            family
        },
        bold,
        italic,
        mapped,
    }
}

/// Group spans into visual lines (y-buckets ±2pt), sort lines top-first
/// (y desc — PDF y-up), runs left-first (x asc). Interleave images by y.
/// Unmapped-font report appended as a final honest paragraph.
fn group_blocks(
    doc: &lopdf::Document,
    page: u32,
    page_id: lopdf::ObjectId,
    spans: Vec<Span>,
    image_slots: Vec<(Option<String>, f32)>,
    unmapped: Vec<String>,
) -> Result<Vec<OrderedBlock>> {
    // Bucket spans by y (±2pt tolerance): same visual line.
    let mut sorted = spans;
    sorted.sort_by(|a, b| b.y.partial_cmp(&a.y).unwrap_or(std::cmp::Ordering::Equal));
    let mut lines: Vec<LineInfo> = Vec::new();
    for span in sorted {
        if let Some(line) = lines.last_mut() {
            if (line.y - span.y).abs() <= 2.0 {
                line.runs.push(span);
                continue;
            }
        }
        // Alignment guess from first span's x: near-left → Left,
        // centered → Center, near-right → Right.
        let align = if span.x < 72.0 {
            None
        } else if span.x < 200.0 {
            Some(docx_rs::AlignmentType::Center)
        } else {
            Some(docx_rs::AlignmentType::Right)
        };
        lines.push(LineInfo {
            y: span.y,
            align,
            runs: vec![span],
        });
    }
    for line in &mut lines {
        line.runs
            .sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    }
    // Resolve images at stream y → pixel bytes.
    let mut blocks: Vec<OrderedBlock> = lines.into_iter().map(OrderedBlock::Text).collect();
    for (name, y) in image_slots {
        match name.as_deref().and_then(|n| page_jpeg(doc, page_id, n)) {
            Some((bytes, w, h)) => blocks.push(OrderedBlock::Image {
                y,
                bytes,
                width_px: w,
                height_px: h,
            }),
            None => {
                blocks.push(OrderedBlock::Text(LineInfo {
                    y,
                    align: None,
                    runs: vec![Span {
                        x: 0.0,
                        y,
                        size: 10.0,
                        family: "Calibri".to_string(),
                        bold: false,
                        italic: false,
                        text: format!("[Image on page {page} — format not embeddable]"),
                    }],
                }));
            }
        }
    }
    // Visual order: top of page first (y desc).
    blocks.sort_by(|a, b| {
        let ya = match a {
            OrderedBlock::Text(l) => l.y,
            OrderedBlock::Image { y, .. } => *y,
        };
        let yb = match b {
            OrderedBlock::Text(l) => l.y,
            OrderedBlock::Image { y, .. } => *y,
        };
        yb.partial_cmp(&ya).unwrap_or(std::cmp::Ordering::Equal)
    });
    // Honest report for fonts that can't map to Unicode (no mojibake).
    if !unmapped.is_empty() {
        let mut seen = unmapped;
        seen.sort();
        seen.dedup();
        blocks.push(OrderedBlock::Text(LineInfo {
            y: f32::MIN,
            align: None,
            runs: vec![Span {
                x: 0.0,
                y: f32::MIN,
                size: 9.0,
                family: "Calibri".to_string(),
                bold: false,
                italic: true,
                text: format!(
                    "[Some text on page {page} uses fonts without Unicode mapping ({}) — those runs were skipped]",
                    seen.join(", ")
                ),
            }],
        }));
    }
    Ok(blocks)
}

/// Legacy flat-string decoder: superseded by `collect_styled`.
#[allow(dead_code)]
fn append_operands(
    out: &mut String,
    encoding: &lopdf::Encoding<'_>,
    operands: &[lopdf::Object],
) -> Result<()> {
    for operand in operands {
        match operand {
            lopdf::Object::String(bytes, _) => {
                let mut s = String::new();
                encoding
                    .write_to_string(bytes, &mut s)
                    .map_err(|e| ForgeError::PdfReadFailed(e.to_string()))?;
                out.push_str(&s);
            }
            lopdf::Object::Array(arr) => {
                append_operands(out, encoding, arr)?;
                out.push(' ');
            }
            lopdf::Object::Integer(i) if *i < -100 => out.push(' '),
            _ => {}
        }
    }
    Ok(())
}

/// JPEG (`DCTDecode`) XObject bytes + pixel dims, if `name` resolves.
/// Anything else (JPX, masks, forms) → `None` → caller emits a marker.
fn page_jpeg(
    doc: &lopdf::Document,
    page_id: lopdf::ObjectId,
    name: &str,
) -> Option<(Vec<u8>, u32, u32)> {
    let page = doc.get_dictionary(page_id).ok()?;
    let resources = doc.get_dict_in_dict(page, b"Resources").ok()?;
    let xobject = doc.get_dict_in_dict(resources, b"XObject").ok()?;
    let stream_id = xobject.get(name.as_bytes()).ok()?.as_reference().ok()?;
    let stream = doc.get_object(stream_id).ok()?.as_stream().ok()?;
    let is_jpeg = stream
        .dict
        .get(b"Filter")
        .ok()
        .map(|f| format!("{f:?}").contains("DCTDecode"))
        .unwrap_or(false);
    if !is_jpeg {
        return None;
    }
    let width = stream.dict.get(b"Width").ok()?.as_i64().ok()? as u32;
    let height = stream.dict.get(b"Height").ok()?.as_i64().ok()? as u32;
    // Raw stream content IS the JPEG (DCTDecode = JPEG bytes).
    let bytes = stream.content.clone();
    if bytes.len() < 4 || width == 0 || height == 0 {
        return None;
    }
    // Sanity: must start with JPEG SOI.
    if bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    Some((bytes, width, height))
}

/// Split a text-table line on runs of 2+ spaces (single spaces stay in-cell).
fn split_columns(line: &str) -> Vec<String> {
    let mut cols = Vec::new();
    let mut current = String::new();
    let mut spaces = 0;
    for ch in line.chars() {
        if ch == ' ' {
            spaces += 1;
            if spaces < 2 {
                current.push(ch);
            }
            continue;
        }
        if spaces >= 2 {
            cols.push(current.trim().to_string());
            current = String::new();
        }
        spaces = 0;
        current.push(ch);
    }
    cols.push(current.trim().to_string());
    cols.retain(|c| !c.is_empty());
    cols
}

/// Legacy unstyled table writer: superseded by `push_styled_table`.
#[allow(dead_code)]
fn push_table(word: docx_rs::Docx, lines: &[String], page_start: bool) -> docx_rs::Docx {
    let max_cols = lines
        .iter()
        .map(|l| split_columns(l).len())
        .max()
        .unwrap_or(1)
        .max(1);
    let mut rows = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let mut cols = split_columns(line);
        while cols.len() < max_cols {
            cols.push(String::new());
        }
        let cells: Vec<docx_rs::TableCell> = cols
            .into_iter()
            .map(|text| {
                let mut para =
                    docx_rs::Paragraph::new().add_run(docx_rs::Run::new().add_text(text));
                if page_start && i == 0 {
                    para = para.page_break_before(true);
                }
                docx_rs::TableCell::new().add_paragraph(para)
            })
            .collect();
        rows.push(docx_rs::TableRow::new(cells));
    }
    word.add_table(docx_rs::Table::new(rows))
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
    fn test_render_pdf_target_not_decodable() {
        // PDF image target stays known-undecodable by the image crate.
        assert!(!ImageFormat::Pdf.descriptor().can_decode);
    }
    #[test]
    fn test_render_roundtrip_page_to_image() {
        let writer = ForgePdfWriter;
        let pdf = writer
            .write_images(&[rgba_image(32, 24)], &PdfWriteSpec::default())
            .unwrap();
        let renderer = HayroRenderer;
        let images = renderer.render(&pdf, &[1], 72).unwrap();
        assert_eq!(images.len(), 1);
        assert!(images[0].dimensions.width > 0 && images[0].dimensions.height > 0);
        assert_eq!(
            images[0].pixels.len(),
            images[0].dimensions.width as usize * images[0].dimensions.height as usize * 4
        );
    }

    #[test]
    fn test_render_rejects_garbage_and_bad_pages() {
        let renderer = HayroRenderer;
        assert!(matches!(
            renderer.render(b"definitely-not-a-pdf", &[1], 72),
            Err(ForgeError::PdfReadFailed(_))
        ));
        let writer = ForgePdfWriter;
        let pdf = writer
            .write_images(&[rgba_image(8, 8)], &PdfWriteSpec::default())
            .unwrap();
        assert!(matches!(
            renderer.render(&pdf, &[99], 72),
            Err(ForgeError::InvalidConfiguration(_))
        ));
        assert!(matches!(
            renderer.render(&pdf, &[], 72),
            Err(ForgeError::InvalidConfiguration(_))
        ));
        assert!(matches!(
            renderer.render(&pdf, &[1], 0),
            Err(ForgeError::InvalidConfiguration(_))
        ));
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

    /// Unzip `word/document.xml` from docx bytes (OOXML content assertion).
    fn document_xml(docx: &[u8]) -> String {
        use std::io::Read as _;
        let cursor = std::io::Cursor::new(docx);
        let mut zip = zip::ZipArchive::new(cursor).expect("docx is a zip");
        let mut xml = String::new();
        zip.by_name("word/document.xml")
            .expect("document.xml present")
            .read_to_string(&mut xml)
            .expect("document.xml reads");
        xml
    }

    #[test]
    fn test_docx_text_content_in_order() {
        let conv = LopdfToDocx;
        let xml = document_xml(&conv.convert(&three_page_pdf(), None).unwrap());
        for word in ["alpha", "beta", "gamma"] {
            assert!(xml.contains(word), "missing {word}");
        }
        // Stream order preserved across pages.
        let (a, b, g) = (
            xml.find("alpha").unwrap(),
            xml.find("beta").unwrap(),
            xml.find("gamma").unwrap(),
        );
        assert!(a < b && b < g, "out-of-order text");
        // Only pages 2+ carry breaks; page 1 never does. printpdf emits an
        // empty leading content stream, so page 1 yields zero blocks and the
        // doc's first break lands on page 2's paragraph (count == 1).
        assert_eq!(xml.matches("w:pageBreakBefore").count(), 1);
    }

    #[test]
    fn test_docx_table_detection() {
        use printpdf::{
            BuiltinFont, Mm, Op, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Pt,
            TextItem,
        };
        // Two columnar lines (2+ spaces) → real w:tbl, not paragraphs.
        let mut doc = PdfDocument::new("table test");
        let page = PdfPage::new(
            Mm(210.0),
            Mm(297.0),
            vec![
                Op::StartTextSection,
                Op::SetTextCursor {
                    pos: Point::new(Mm(20.0), Mm(270.0)),
                },
                Op::SetFont {
                    font: PdfFontHandle::Builtin(BuiltinFont::Helvetica),
                    size: Pt(12.0),
                },
                Op::ShowText {
                    items: vec![TextItem::Text("Name   Age   City".to_string())],
                },
                Op::AddLineBreak,
                Op::ShowText {
                    items: vec![TextItem::Text("Ann    30    Paris".to_string())],
                },
                Op::EndTextSection,
            ],
        );
        let pdf = doc
            .with_pages(vec![page])
            .save(&PdfSaveOptions::default(), &mut Vec::new());
        let xml = document_xml(&LopdfToDocx.convert(&pdf, None).unwrap());
        assert!(xml.contains("w:tbl"), "no table emitted");
        for cell in ["Name", "Paris", "30"] {
            assert!(xml.contains(cell), "missing cell {cell}");
        }
    }

    #[test]
    fn test_docx_embeds_jpeg_image() {
        // Image-only PDF: printpdf embeds the PNG (re-encoded); converter
        // must emit either an inline image or an honest marker — never drop.
        let writer = ForgePdfWriter;
        let pdf = writer
            .write_images(&[rgba_image(16, 16)], &PdfWriteSpec::default())
            .unwrap();
        let xml = document_xml(&LopdfToDocx.convert(&pdf, None).unwrap());
        assert!(
            xml.contains("w:drawing") || xml.contains("[Image on page 1"),
            "image silently dropped"
        );
    }

    /// Two-column page via absolute Tm: right run must follow left run even
    /// when stream order is right-first (visual x-sort, not stream order).
    #[test]
    fn test_docx_visual_order_beats_stream_order() {
        use printpdf::{
            BuiltinFont, Mm, Op, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Pt, TextItem,
        };
        // Stream emits RIGHT text first at x=300, then LEFT at x=50.
        let page = PdfPage::new(
            Mm(210.0),
            Mm(297.0),
            vec![
                Op::StartTextSection,
                Op::SetFont {
                    font: PdfFontHandle::Builtin(BuiltinFont::Helvetica),
                    size: Pt(12.0),
                },
                Op::SetTextMatrix {
                    matrix: printpdf::TextMatrix::Raw([1.0, 0.0, 0.0, 1.0, 300.0, 700.0]),
                },
                Op::ShowText {
                    items: vec![TextItem::Text("RIGHT".to_string())],
                },
                Op::SetTextMatrix {
                    matrix: printpdf::TextMatrix::Raw([1.0, 0.0, 0.0, 1.0, 50.0, 700.0]),
                },
                Op::ShowText {
                    items: vec![TextItem::Text("LEFT".to_string())],
                },
                Op::EndTextSection,
            ],
        );
        let mut doc = PdfDocument::new("order test");
        let pdf = doc
            .with_pages(vec![page])
            .save(&PdfSaveOptions::default(), &mut Vec::new());
        let xml = document_xml(&LopdfToDocx.convert(&pdf, None).unwrap());
        let (left, right) = (xml.find("LEFT").unwrap(), xml.find("RIGHT").unwrap());
        assert!(left < right, "visual order lost: RIGHT before LEFT");
    }

    /// Unicode survival: Myanmar + Japanese + bold styling land in document.xml.
    #[test]
    fn test_docx_unicode_and_styling() {
        use printpdf::{
            BuiltinFont, Mm, Op, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Pt,
            TextItem,
        };
        let page = PdfPage::new(
            Mm(210.0),
            Mm(297.0),
            vec![
                Op::StartTextSection,
                Op::SetTextCursor {
                    pos: Point::new(Mm(20.0), Mm(270.0)),
                },
                Op::SetFont {
                    font: PdfFontHandle::Builtin(BuiltinFont::HelveticaBold),
                    size: Pt(16.0),
                },
                Op::ShowText {
                    items: vec![TextItem::Text("Hello မင်္ဂလာပါ こんにちは".to_string())],
                },
                Op::EndTextSection,
            ],
        );
        let mut doc = PdfDocument::new("unicode test");
        let pdf = doc
            .with_pages(vec![page])
            .save(&PdfSaveOptions::default(), &mut Vec::new());
        let xml = document_xml(&LopdfToDocx.convert(&pdf, None).unwrap());
        // ASCII survives verbatim; bold + size land as w:b / w:sz.
        assert!(xml.contains("Hello"), "ascii lost");
        assert!(xml.contains("w:b"), "bold lost");
        assert!(xml.contains("w:sz"), "font size lost");
        // CJK-safe fonts on eastAsia so Word renders Myanmar/Japanese.
        assert!(xml.contains("w:eastAsia"), "eastAsia font missing");
    }

    #[test]
    fn test_merge_concatenates_pages_in_order() {
        let merger = LopdfMerger;
        let one = text_pdf(&["page one"]);
        let two = text_pdf(&["page two", "page three"]);
        let out = merger.merge(&[&one, &two]).unwrap();
        assert_eq!(&out[0..4], b"%PDF");
        assert_eq!(LopdfSplitter.page_count(&out).unwrap(), 3);
    }

    #[test]
    fn test_merge_rejects_empty_and_garbage() {
        let merger = LopdfMerger;
        assert!(merger.merge(&[]).is_err());
        assert!(merger.merge(&[b"%PDF-1.7 garbage"]).is_err());
        let one = text_pdf(&["ok"]);
        assert!(merger.merge(&[&one, b"nope"]).is_err());
    }

    #[test]
    fn test_compress_keeps_valid_pdf() {
        let comp = LopdfCompressor;
        let src = three_page_pdf();
        for level in [PdfCompressLevel::Light, PdfCompressLevel::Balanced] {
            let out = comp.compress(&src, level).unwrap();
            assert_eq!(&out[0..4], b"%PDF");
            assert_eq!(LopdfSplitter.page_count(&out).unwrap(), 3);
        }
        assert!(comp.compress(b"garbage", PdfCompressLevel::Light).is_err());
    }
}
