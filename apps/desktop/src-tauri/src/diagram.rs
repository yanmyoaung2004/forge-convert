//! SVG Diagram file commands: thin persistence over `StdFileSystem`.
//!
//! The diagram document itself lives in TypeScript (`diagram/types.ts`);
//! Rust only moves bytes (atomic write / bounded read), same as CLI file
//! ops. Validation of the JSON shape happens in TS (`validateDoc`).

use std::path::PathBuf;

use forge_core::{FileSystem as _, ForgeError};
use forge_engine::StdFileSystem;
use serde::{Deserialize, Serialize};

use super::commands::CommandError;

type DiagramResult<T> = Result<T, CommandError>;

/// `save_diagram` — write project JSON atomically (creates parent dirs).
#[derive(Debug, Deserialize)]
pub(crate) struct SaveDiagramArgs {
    path: String,
    json: String,
}

/// `load_diagram` — read project JSON (bounded by `StdFileSystem`).
#[derive(Debug, Serialize)]
pub(crate) struct LoadDiagramDone {
    path: String,
    json: String,
}

#[tauri::command]
pub fn save_diagram(args: SaveDiagramArgs) -> DiagramResult<String> {
    if args.json.len() > 8 * 1024 * 1024 {
        return Err(ForgeError::ResourceLimitExceeded(format!(
            "diagram JSON is {} bytes (limit 8 MiB)",
            args.json.len()
        ))
        .into());
    }
    let path = PathBuf::from(&args.path);
    StdFileSystem
        .write_atomic(&path, args.json.as_bytes())
        .map_err(CommandError::from)?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub fn load_diagram(path: String) -> DiagramResult<LoadDiagramDone> {
    let fs_path = PathBuf::from(&path);
    let bytes = StdFileSystem.read(&fs_path).map_err(CommandError::from)?;
    let json = String::from_utf8(bytes)
        .map_err(|e| ForgeError::InvalidFile(format!("diagram is not UTF-8: {e}")))?;
    Ok(LoadDiagramDone {
        path,
        json: json.clone(),
    })
}

/// `export_svg_file` — write generated SVG atomically (TS serializes).
#[derive(Debug, Deserialize)]
pub(crate) struct ExportSvgArgs {
    path: String,
    svg: String,
}

#[tauri::command]
pub fn export_svg_file(args: ExportSvgArgs) -> DiagramResult<String> {
    if !args.svg.trim_start().starts_with("<svg") {
        return Err(ForgeError::InvalidConfiguration(
            "refusing to write non-SVG content with .svg path".to_string(),
        )
        .into());
    }
    let path = PathBuf::from(&args.path);
    StdFileSystem
        .write_atomic(&path, args.svg.as_bytes())
        .map_err(CommandError::from)?;
    Ok(path.display().to_string())
}
