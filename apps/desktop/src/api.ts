// Shared Tauri invoke wrappers — the ONLY bridge to Rust.
// Backend is authoritative for capabilities, validation, and conversion.
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
export interface FormatInfo {
  id: string;
  name: string;
  extensions: string[];
  mime_types: string[];
  can_decode: boolean;
  can_encode: boolean;
  supports_alpha: boolean;
}

export interface FileInfo {
  name: string;
  format: string;
  mime_type: string;
  width: number | null;
  height: number | null;
  pixel: string | null;
  alpha: boolean;
  size_bytes: number;
  /** PDF page count (null for images / unreadable PDFs). */
  pages: number | null;
}
export interface ConvertDone {
  outputs: string[];
  skipped: string[];
  failures: string[];
  sizes: FileSizes[];
}

export interface FileSizes {
  output: string;
  input_bytes: number;
  output_bytes: number;
}

export interface PresetInfo {
  key: string;
  name: string;
  format: string;
  quality: number;
}

export interface HistoryRow {
  job_id: string;
  operation: string;
  input: string;
  output: string;
  format: string;
  status: string;
  duration_ms: number;
}

export interface CommandError {
  kind: string;
  message: string;
}

export function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    "message" in value
  );
}

/** Native file picker (multiple images + PDFs). Null when cancelled. */
export async function pickFiles(): Promise<string[] | null> {
  const selected = await open({
    multiple: true,
    filters: [
      {
        name: "Images & PDF",
        extensions: ["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "pdf"],
      },
    ],
  });
  if (selected === null) return null;
  return Array.isArray(selected) ? selected : [selected];
}

/** Native directory picker. Null when cancelled. */
export async function pickDirectory(): Promise<string | null> {
  const selected = await open({ multiple: false, directory: true });
  if (selected === null || Array.isArray(selected)) return null;
  return selected;
}

/** Reveal a converted file in the OS file explorer (selects it). */
export async function revealInFolder(path: string): Promise<void> {
  await revealItemInDir(path);
}
export const api = {
  formats: () => invoke<FormatInfo[]>("get_format_capabilities"),
  fileInfo: (path: string) => invoke<FileInfo>("get_file_info", { path }),
  convert: (args: {
    inputs: string[];
    to: string;
    outputDir?: string;
    quality?: number;
    maxWidth?: number;
    maxHeight?: number;
    width?: number;
    height?: number;
    fit?: string;
    fill?: string;
    filter?: string;
    upscale?: boolean;
    pngLevel?: number;
    webpLossless?: boolean;
    stripMetadata?: boolean;
    onCollision?: string;
  }) =>
    invoke<ConvertDone>("convert_image", {
      args: {
        inputs: args.inputs,
        to: args.to,
        output_dir: args.outputDir ?? null,
        quality: args.quality ?? null,
        max_width: args.maxWidth ?? null,
        max_height: args.maxHeight ?? null,
        width: args.width ?? null,
        height: args.height ?? null,
        fit: args.fit ?? null,
        fill: args.fill ?? null,
        filter: args.filter ?? null,
        upscale: args.upscale ?? null,
        png_level: args.pngLevel ?? null,
        webp_lossless: args.webpLossless ?? null,
        strip_metadata: args.stripMetadata ?? null,
        on_collision: args.onCollision ?? null,
      },
    }),
  imagesToPdf: (args: {
    inputs: string[];
    output: string;
    page?: string;
    landscape?: boolean;
  }) =>
    invoke<string>("convert_images_to_pdf", {
      args: {
        inputs: args.inputs,
        output: args.output,
        page: args.page ?? null,
        landscape: args.landscape ?? null,
      },
    }),
  /** Split PDF pages (same `1-3` / `1,3,5-7` grammar as CLI). */
  splitPdf: (args: { input: string; pages: string; output?: string }) =>
    invoke<string>("split_pdf", {
      args: {
        input: args.input,
        pages: args.pages,
        output: args.output ?? null,
        on_collision: "rename",
      },
    }),
  /** Export PDF text to Word (.docx, text-only). */
  pdfToDocx: (args: { input: string; pages?: string; output?: string }) =>
    invoke<string>("pdf_to_docx", {
      args: {
        input: args.input,
        pages: args.pages ?? null,
        output: args.output ?? null,
        on_collision: "rename",
      },
    }),
  /** Total PDF pages (drives the desktop page-count line). */
  pdfPageCount: (input: string) => invoke<number>("pdf_page_count", { input }),
  /** Merge PDFs in order (pages concatenated). */
  mergePdfs: (args: { inputs: string[]; output?: string }) =>
    invoke<string>("merge_pdfs", {
      args: {
        inputs: args.inputs,
        output: args.output ?? null,
        on_collision: "rename",
      },
    }),
  /** Compress a PDF (light = prune orphans, balanced = + recompress). */
  compressPdf: (args: { input: string; level?: string; output?: string }) =>
    invoke<string>("compress_pdf", {
      args: {
        input: args.input,
        level: args.level ?? null,
        output: args.output ?? null,
        on_collision: "rename",
      },
    }),
  saveDiagram: (path: string, json: string) => invoke<string>("save_diagram", { args: { path, json } }),
  /** Load diagram project JSON. Validate with validateDoc (TS side). */
  loadDiagram: (path: string) => invoke<{ path: string; json: string }>("load_diagram", { path }),
  /** Write generated SVG string to disk (refuses non-SVG content). */
  exportSvg: (path: string, svg: string) => invoke<string>("export_svg_file", { args: { path, svg } }),
  /** Favicon set: favicon.ico + sized PNGs + link snippet. */
  favicon: (args: { input: string; outputDir?: string; sizes?: string }) =>
    invoke<{ outputs: string[]; snippet: string }>("favicon", {
      args: {
        input: args.input,
        output_dir: args.outputDir ?? null,
        sizes: args.sizes ?? null,
        on_collision: "rename",
      },
    }),
  /** Render text as QR PNG/SVG (written to OS temp dir, returns path). */
  qrPng: (args: { text: string; size?: number; ec?: string; format?: string; quiet?: boolean; dark?: string; light?: string; logoPath?: string | null }) =>
    invoke<string>("qr_png", { args: { text: args.text, size: args.size ?? null, ec: args.ec ?? null, format: args.format ?? null, quiet: args.quiet ?? null, dark: args.dark ?? null, light: args.light ?? null, logo_path: args.logoPath ?? null } }),
  /** SHA-256 hex of a file (streams in Rust, 512 MiB cap). */
  hashFile: (path: string) => invoke<string>("hash_file", { path }),
  /** Decode a QR code from an image file (returns its text). */
  qrDecode: (path: string) => invoke<string>("qr_decode", { path }),
  presets: () => invoke<PresetInfo[]>("list_presets"),
  history: (limit?: number) =>
    invoke<HistoryRow[]>("get_history", { limit: limit ?? null }),
  cancel: () => invoke<string>("cancel_convert"),
};
