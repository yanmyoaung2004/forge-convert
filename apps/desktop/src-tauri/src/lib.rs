//! ForgeConvert desktop shell: thin Tauri presentation adapter.
//!
//! Job-level commands only (spec §20 / ADR 001+010): the Vue frontend
//! calls `create_conversion_job`-style operations; the engine stays
//! authoritative. No conversion logic lives here or in TypeScript.

mod commands;
mod diagram;
/// Desktop entry point (CTA-conventional: `main.rs` calls `run()`).
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_format_capabilities,
            commands::get_file_info,
            commands::convert_image,
            commands::cancel_convert,
            commands::convert_images_to_pdf,
            commands::split_pdf,
            commands::pdf_to_docx,
            commands::pdf_page_count,
            commands::merge_pdfs,
            commands::compress_pdf,
            commands::render_pdf,
            commands::favicon,
            commands::list_presets,
            commands::get_history,
            diagram::save_diagram,
            diagram::load_diagram,
            diagram::export_svg_file,
            diagram::export_png_file,
            diagram::qr_png,
            diagram::qr_decode,
            diagram::hash_file,
        ])
        .run(tauri::generate_context!())
        .expect("ForgeConvert desktop failed to start");
}
