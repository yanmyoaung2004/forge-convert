//! ForgeConvert desktop shell: thin Tauri presentation adapter.
//!
//! Job-level commands only (spec §20 / ADR 001+010): the Vue frontend
//! calls `create_conversion_job`-style operations; the engine stays
//! authoritative. No conversion logic lives here or in TypeScript.

mod commands;

/// Desktop entry point (CTA-conventional: `main.rs` calls `run()`).
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_format_capabilities,
            commands::get_file_info,
            commands::convert_image,
            commands::convert_images_to_pdf,
            commands::list_presets,
            commands::get_history,
        ])
        .run(tauri::generate_context!())
        .expect("ForgeConvert desktop failed to start");
}
