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
}

export interface ConvertDone {
  outputs: string[];
  skipped: string[];
  failures: string[];
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
  presets: () => invoke<PresetInfo[]>("list_presets"),
  history: (limit?: number) =>
    invoke<HistoryRow[]>("get_history", { limit: limit ?? null }),
};
