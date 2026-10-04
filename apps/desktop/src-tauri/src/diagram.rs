//! SVG Diagram file commands: thin persistence over `StdFileSystem`.
//!
//! The diagram document itself lives in TypeScript (`diagram/types.ts`);
//! Rust only moves bytes (atomic write / bounded read), same as CLI file
//! ops. Validation of the JSON shape happens in TS (`validateDoc`).

use std::path::PathBuf;

use forge_core::{FileSystem as _, ForgeError};
use forge_engine::StdFileSystem;
use forge_image::ForgeImageEncoder;
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

/// `export_png_file` — write canvas-rasterized PNG bytes (base64 from TS) atomically.
/// Same thin-bytes pattern as `export_svg_file`; PNG validation is magic-byte check.
#[derive(Debug, Deserialize)]
pub(crate) struct ExportPngArgs {
    path: String,
    png_base64: String,
}

#[tauri::command]
pub fn export_png_file(args: ExportPngArgs) -> DiagramResult<String> {
    // Minimal base64 decode without a new dep (canvas PNGs are small).
    let bytes = decode_base64(args.png_base64.as_bytes())
        .map_err(|e| ForgeError::InvalidConfiguration(format!("bad png base64: {e}")))?;
    if bytes.len() < 8 || &bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(ForgeError::InvalidConfiguration(
            "refusing to write non-PNG content with .png path".to_string(),
        )
        .into());
    }
    let path = PathBuf::from(&args.path);
    StdFileSystem
        .write_atomic(&path, &bytes)
        .map_err(CommandError::from)?;
    Ok(path.display().to_string())
}

/// Tiny base64 decoder (standard alphabet, `=` padding tolerated, whitespace ignored).
fn decode_base64(input: &[u8]) -> std::result::Result<Vec<u8>, String> {
    const TABLE: &[u8; 128] = &{
        let mut t = [255u8; 128];
        let mut i = 0u8;
        while i < 26 {
            t[(b'A' + i) as usize] = i;
            t[(b'a' + i) as usize] = 26 + i;
            i += 1;
        }
        let mut j = 0u8;
        while j < 10 {
            t[(b'0' + j) as usize] = 52 + j;
            j += 1;
        }
        t[b'+' as usize] = 62;
        t[b'/' as usize] = 63;
        t
    };
    let mut buf: u32 = 0;
    let mut bits = 0u8;
    let mut out = Vec::new();
    for byte in input.iter().copied() {
        if byte == b'=' || byte == b'\n' || byte == b'\r' || byte == b' ' || byte == b'\t' {
            continue;
        }
        if byte >= 128 || TABLE[byte as usize] == 255 {
            return Err(format!("bad char {byte:#x}"));
        }
        buf = (buf << 6) | u32::from(TABLE[byte as usize]);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

/// `qr_png` — render text as QR (PNG bytes or SVG text), written to the OS temp dir.
/// Returns the written path (UI reveals it). Shared `ForgeImageEncoder::encode_qr`
/// (single source with the CLI); unique filename per generate via content hash.
#[derive(Debug, Deserialize)]
pub(crate) struct QrArgs {
    text: String,
    size: Option<u32>,
    ec: Option<String>,
    format: Option<String>,
    quiet: Option<bool>,
    dark: Option<String>,
    light: Option<String>,
    logo_path: Option<String>,
}

#[tauri::command]
pub fn qr_png(args: QrArgs) -> DiagramResult<String> {
    use forge_image::{QrOutput, QrStyle};
    let size = args.size.unwrap_or(256).clamp(128, 1024);
    let quiet = args.quiet.unwrap_or(true);
    let style =
        QrStyle::parse(args.dark.as_deref(), args.light.as_deref()).map_err(CommandError::from)?;
    let logo_bytes = args
        .logo_path
        .as_ref()
        .map(|p| StdFileSystem.read(&PathBuf::from(p)))
        .transpose()
        .map_err(CommandError::from)?;
    let out = ForgeImageEncoder::encode_qr(
        &args.text,
        args.ec.as_deref(),
        args.format.as_deref(),
        size,
        quiet,
        style,
        logo_bytes.as_deref(),
    )
    .map_err(CommandError::from)?;
    // Unique temp name: content hash avoids same-tick collisions + repeat overwrites.
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in args.text.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let (ext, bytes) = match &out {
        QrOutput::Png(png) => ("png", png.clone()),
        QrOutput::Svg(svg) => ("svg", svg.as_bytes().to_vec()),
    };
    let path = std::env::temp_dir().join(format!("forgeconvert-qr-{hash:016x}-{size}.{ext}"));
    StdFileSystem
        .write_atomic(&path, &bytes)
        .map_err(CommandError::from)?;
    Ok(path.display().to_string())
}
/// `qr_decode` — read a QR code from an image file, return its text.
/// Shared `ForgeImageEncoder::decode_qr` (single source with the CLI).
/// No QR in image → `InvalidFile`; undecodable pixels → `DecodeFailed`.
#[tauri::command]
pub fn qr_decode(path: String) -> DiagramResult<String> {
    let fs_path = PathBuf::from(&path);
    let bytes = StdFileSystem.read(&fs_path).map_err(CommandError::from)?;
    ForgeImageEncoder::decode_qr(&bytes).map_err(CommandError::from)
}

/// `hash_file` — streaming SHA-256 of a file (hex). Bounded reads via
/// `StdFileSystem` (512 MiB cap); SubtleCrypto can't reach files.
#[tauri::command]
pub fn hash_file(path: String) -> DiagramResult<String> {
    use sha2::{Digest, Sha256};
    let fs_path = PathBuf::from(&path);
    let bytes = StdFileSystem.read(&fs_path).map_err(CommandError::from)?;
    // Chunked update (constant hasher memory even at the 512 MiB cap).
    let mut hasher = Sha256::new();
    for chunk in bytes.chunks(64 * 1024) {
        hasher.update(chunk);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
