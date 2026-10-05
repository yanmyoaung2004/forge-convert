<!-- ForgeConvert shell: dark sidebar workbench. Contract:
THESIS: calm dark workbench, files flow left→right (queue → options → action → results).
OWN-WORLD: charcoal #12100e, paper #f3ede3, ember #e86a2c; hairline borders; 12px cards.
STORY: drop files → pick format → Convert → per-file savings. FIRST VIEWPORT: rail + queue + options + action.
FORM: Linear-grade Operate shell. FINISH: unreviewed and undocumented is unfinished. -->
<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import DiagramTab from "./diagram/DiagramTab.vue";
import ToolsTab from "./tools/ToolsTab.vue";
import {
  api,
  isCommandError,
  pickDirectory,
  pickFiles,
  revealInFolder,
  type ConvertDone,
  type FileInfo,
  type FormatInfo,
  type HistoryRow,
  type PresetInfo,
} from "./api";

// -- backend-driven state (no business logic in TS) --------------------------
const formats = ref<FormatInfo[]>([]);
const presets = ref<PresetInfo[]>([]);
const files = ref<string[]>([]);
const infos = ref<FileInfo[]>([]);
const target = ref("webp");
const quality = ref(80);
const stripMetadata = ref(true);
const resizeMode = ref<"off" | "exact" | "fit" | "fill">("off");
const exactW = ref<number | null>(800);
const exactH = ref<number | null>(600);
const fitBox = ref("800x600");
const fillBox = ref("800x600");
const filter = ref("lanczos3");
const upscale = ref(false);
const pngLevel = ref(6);
const webpLossless = ref(false);
const outputDir = ref<string | null>(null);
const busy = ref(false);
const cancelling = ref(false);
const progress = ref<string | null>(null);
const progressFraction = ref(0);
const result = ref<ConvertDone | null>(null);
const error = ref<string | null>(null);
const backendError = ref<string | null>(null);
const history = ref<HistoryRow[]>([]);
const tab = ref<"convert" | "pdf" | "diagram" | "tools" | "history">("convert");
// -- PDF tab state (backend authoritative; TS holds no business logic) -----
const pdfFile = ref<string | null>(null);
const pdfPages = ref<number | null>(null);
const pdfRange = ref("1-2");
const pdfBusy = ref(false);
const pdfResult = ref<string | null>(null);
const pdfCompressLevel = ref("balanced");
const pdfRenderDpi = ref(200);
const pdfRenderFormat = ref("png");
const mergeFiles = ref<string[]>([]);
const revealFailed = ref<string | null>(null);
const dragActive = ref(false);
let dragDepth = 0;
const encodable = computed(() => formats.value.filter((f) => f.can_encode));
const backendReady = computed(() => formats.value.length > 0);
const canConvert = computed(
  () => backendReady.value && !busy.value && files.value.length > 0 && target.value.length > 0,
);
const tabMeta: Record<string, { title: string; hint: string }> = {
  convert: { title: "Convert", hint: "Images → modern formats" },
  pdf: { title: "PDF", hint: "Split, merge, render, Word" },
  diagram: { title: "Diagram", hint: "Visual editor" },
  tools: { title: "Tools", hint: "QR, text & data" },
  history: { title: "History", hint: "Recent jobs" },
};
const railItems = [
  { id: "convert", label: "Convert" },
  { id: "pdf", label: "PDF" },
  { id: "diagram", label: "Diagram" },
  { id: "tools", label: "Tools" },
  { id: "history", label: "History" },
] as const;

function fileName(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

function parentDir(path: string): string {
  const parts = path.split(/[/\\]/);
  parts.pop();
  return parts.join("/") || path;
}

async function refresh() {
  backendError.value = null;
  try {
    formats.value = await api.formats();
    presets.value = await api.presets();
    history.value = await api.history(20);
  } catch (err) {
    backendError.value = isCommandError(err)
      ? `${err.kind}: ${err.message}`
      : "Backend unreachable — is the Tauri shell running? (pnpm tauri dev)";
    return;
  }
  if (!encodable.value.some((f) => f.id === target.value)) {
    target.value = encodable.value[0]?.id ?? "webp";
  }
}
async function addFiles() {
  const picked = await pickFiles();
  if (!picked) return;
  for (const path of picked) {
    if (!files.value.includes(path)) files.value.push(path);
  }
  await inspectAll();
}

const IMAGE_EXTS = new Set(["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "pdf"]);

function extOf(path: string): string {
  return path.split(".").pop()?.toLowerCase() ?? "";
}

async function addPaths(paths: string[]) {
  let skipped = 0;
  for (const path of paths) {
    if (!IMAGE_EXTS.has(extOf(path))) {
      skipped += 1;
      continue;
    }
    if (!files.value.includes(path)) files.value.push(path);
  }
  if (skipped > 0) {
    error.value = `${skipped} dropped file${skipped === 1 ? "" : "s"} skipped (not an image/PDF)`;
  }
  await inspectAll();
}

function onDragEnter(event: DragEvent) {
  if (!event.dataTransfer?.types.includes("Files")) return;
  dragDepth += 1;
  dragActive.value = true;
}

function onDragLeave() {
  dragDepth = Math.max(0, dragDepth - 1);
  if (dragDepth === 0) dragActive.value = false;
}

function onDragOver(event: DragEvent) {
  if (dragActive.value) event.preventDefault();
}

async function onDrop(event: DragEvent) {
  event.preventDefault();
  dragDepth = 0;
  dragActive.value = false;
  const dropped = event.dataTransfer?.files;
  if (!dropped || dropped.length === 0) return;
  const paths: string[] = [];
  for (const file of dropped) {
    const full = (file as unknown as { path?: string }).path;
    paths.push(full ?? file.name);
  }
  await addPaths(paths);
}

async function inspectAll() {
  infos.value = [];
  for (const path of files.value) {
    try {
      infos.value.push(await api.fileInfo(path));
    } catch {
      infos.value.push({
        name: fileName(path),
        format: "Unknown",
        mime_type: "",
        width: null,
        height: null,
        pixel: null,
        alpha: false,
        size_bytes: 0,
        pages: null,
      });
    }
  }
}

function removeFile(path: string) {
  files.value = files.value.filter((f) => f !== path);
  void inspectAll();
}

async function chooseOutputDir() {
  outputDir.value = await pickDirectory();
}

async function pickPdf() {
  const picked = await pickFiles();
  if (!picked || picked.length === 0) return;
  const pdf = picked.find((p) => extOf(p) === "pdf") ?? picked[0];
  pdfFile.value = pdf;
  pdfPages.value = null;
  pdfResult.value = null;
  try {
    pdfPages.value = await api.pdfPageCount(pdf);
  } catch {
    pdfPages.value = null;
  }
}

async function runPdfSplit() {
  if (!pdfFile.value || pdfBusy.value) return;
  pdfBusy.value = true;
  error.value = null;
  pdfResult.value = null;
  try {
    pdfResult.value = await api.splitPdf({ input: pdfFile.value, pages: pdfRange.value });
    history.value = await api.history(20);
  } catch (err) {
    error.value = isCommandError(err) ? `${err.kind}: ${err.message}` : String(err);
  } finally {
    pdfBusy.value = false;
  }
}

async function runPdfToDocx() {
  if (!pdfFile.value || pdfBusy.value) return;
  pdfBusy.value = true;
  error.value = null;
  pdfResult.value = null;
  try {
    pdfResult.value = await api.pdfToDocx({
      input: pdfFile.value,
      pages: pdfRange.value.trim() ? pdfRange.value : undefined,
    });
    history.value = await api.history(20);
  } catch (err) {
    error.value = isCommandError(err) ? `${err.kind}: ${err.message}` : String(err);
  } finally {
    pdfBusy.value = false;
  }
}

async function pickMergeFiles() {
  const picked = await pickFiles();
  if (!picked || picked.length === 0) return;
  const pdfs = picked.filter((p) => extOf(p) === "pdf");
  if (pdfs.length === 0) {
    error.value = "No PDFs in selection — pick .pdf files to merge.";
    return;
  }
  mergeFiles.value = pdfs;
  pdfResult.value = null;
}

async function runPdfMerge() {
  if (mergeFiles.value.length === 0 || pdfBusy.value) return;
  pdfBusy.value = true;
  error.value = null;
  pdfResult.value = null;
  try {
    pdfResult.value = await api.mergePdfs({ inputs: mergeFiles.value });
    history.value = await api.history(20);
  } catch (err) {
    error.value = isCommandError(err) ? `${err.kind}: ${err.message}` : String(err);
  } finally {
    pdfBusy.value = false;
  }
}

async function runPdfCompress() {
  if (!pdfFile.value || pdfBusy.value) return;
  pdfBusy.value = true;
  error.value = null;
  pdfResult.value = null;
  try {
    pdfResult.value = await api.compressPdf({ input: pdfFile.value, level: pdfCompressLevel.value });
    history.value = await api.history(20);
  } catch (err) {
    error.value = isCommandError(err) ? `${err.kind}: ${err.message}` : String(err);
  } finally {
    pdfBusy.value = false;
  }
}
async function runPdfRender() {
  if (!pdfFile.value || pdfBusy.value) return;
  pdfBusy.value = true;
  error.value = null;
  pdfResult.value = null;
  try {
    const paths = await api.renderPdf({
      input: pdfFile.value,
      pages: pdfRange.value.trim() ? pdfRange.value : undefined,
      dpi: pdfRenderDpi.value,
      format: pdfRenderFormat.value,
    });
    pdfResult.value = paths[0] ?? null;
    history.value = await api.history(20);
  } catch (err) {
    error.value = isCommandError(err) ? `${err.kind}: ${err.message}` : String(err);
  } finally {
    pdfBusy.value = false;
  }
}
async function convert() {
  if (!canConvert.value) return;
  busy.value = true;
  error.value = null;
  progress.value = `Converting ${files.value.length} file${files.value.length === 1 ? "" : "s"}…`;
  try {
    result.value = await api.convert({
      inputs: files.value,
      to: target.value,
      outputDir: outputDir.value ?? undefined,
      quality: quality.value,
      width: resizeMode.value === "exact" ? (exactW.value ?? undefined) : undefined,
      height: resizeMode.value === "exact" ? (exactH.value ?? undefined) : undefined,
      fit: resizeMode.value === "fit" ? fitBox.value : undefined,
      fill: resizeMode.value === "fill" ? fillBox.value : undefined,
      filter: filter.value,
      upscale: resizeMode.value === "off" ? undefined : upscale.value,
      pngLevel: target.value === "png" ? pngLevel.value : undefined,
      webpLossless: target.value === "webp" ? webpLossless.value || undefined : undefined,
      onCollision: "rename",
    });
    history.value = await api.history(20);
  } catch (err) {
    error.value = isCommandError(err)
      ? `${err.kind}: ${err.message}`
      : String(err);
  } finally {
    busy.value = false;
    progress.value = null;
  }
}

async function reveal(path: string) {
  revealFailed.value = null;
  try {
    await revealInFolder(path);
  } catch {
    revealFailed.value = `Could not open folder for ${fileName(path)}`;
  }
}

async function cancelConvert() {
  if (!busy.value || cancelling.value) return;
  cancelling.value = true;
  try {
    await api.cancel();
  } catch {
    // Convert is a single round-trip today: cancellation lands when the
    // backend runs conversions as cancellable jobs (slice 4, phase 2).
    // Until then this is a no-op that keeps the button honest.
  } finally {
    cancelling.value = false;
  }
}

const faviconSnippet = ref<string | null>(null);

async function runFavicon() {
  if (files.value.length === 0 || busy.value) return;
  busy.value = true;
  error.value = null;
  faviconSnippet.value = null;
  try {
    const done = await api.favicon({ input: files.value[0], outputDir: outputDir.value ?? undefined });
    faviconSnippet.value = `Wrote ${done.outputs.length} files:\n${done.outputs.join("\n")}\n\n${done.snippet}`;
    history.value = await api.history(20);
  } catch (err) {
    error.value = isCommandError(err) ? `${err.kind}: ${err.message}` : String(err);
  } finally {
    busy.value = false;
  }
}
function savingsText(inputBytes: number, outputBytes: number): string {
  if (inputBytes <= 0) return formatBytes(outputBytes);
  const saved = ((inputBytes - outputBytes) / inputBytes) * 100;
  return `${formatBytes(inputBytes)} → ${formatBytes(outputBytes)} · ${saved.toFixed(1)}% saved`;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function sizeFor(output: string): { input_bytes: number; output_bytes: number } | undefined {
  return result.value?.sizes.find((s) => s.output === output);
}

function applyPreset(key: string) {
  const preset = presets.value.find((p) => p.key === key);
  if (!preset) return;
  target.value = preset.format === "jpg" ? "jpg" : preset.format;
  quality.value = preset.quality;
}

onMounted(() => {
  void refresh();
  void import("@tauri-apps/api/window")
    .then(async ({ getCurrentWindow }) => {
      await getCurrentWindow().onDragDropEvent((event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") {
          dragActive.value = true;
        } else if (event.payload.type === "drop") {
          dragActive.value = false;
          dragDepth = 0;
          void addPaths(event.payload.paths);
        } else {
          dragActive.value = false;
          dragDepth = 0;
        }
      });
    })
    .catch(() => undefined);
});
</script>

<template>
  <main class="app">
    <aside class="rail" aria-label="ForgeConvert workspaces">
      <div class="rail-brand" aria-hidden="true"><span class="rail-mark">F</span></div>
      <nav class="rail-nav" role="tablist" aria-label="Workspaces">
        <button
          v-for="item in railItems"
          :key="item.id"
          role="tab"
          :aria-selected="tab === item.id"
          :class="{ active: tab === item.id }"
          @click="tab = item.id"
        >
          <svg viewBox="0 0 20 20" aria-hidden="true" class="rail-icon">
            <g v-if="item.id === 'convert'" fill="none" stroke="currentColor" stroke-width="1.6">
              <path d="M10 2.5 17 6.5v7L10 17.5 3 13.5v-7L10 2.5Z" stroke-linejoin="round" />
              <path d="M7.5 10h5M10 7.5v5" stroke-linecap="round" />
            </g>
            <g v-else-if="item.id === 'pdf'" fill="none" stroke="currentColor" stroke-width="1.6">
              <path d="M5 2.5h6l4 4v11H5V2.5Z" stroke-linejoin="round" />
              <path d="M11 2.5v4h4M7.5 12h5M7.5 14.5h5" stroke-linecap="round" />
            </g>
            <g v-else-if="item.id === 'diagram'" fill="none" stroke="currentColor" stroke-width="1.6">
              <rect x="2.5" y="2.5" width="6" height="6" rx="1.5" />
              <rect x="11.5" y="11.5" width="6" height="6" rx="1.5" />
              <path d="M8.5 5.5h4a2 2 0 0 1 2 2v4" stroke-linecap="round" />
            </g>
            <g v-else-if="item.id === 'tools'" fill="none" stroke="currentColor" stroke-width="1.6">
              <path d="M12.5 3.5a4 4 0 0 0-5.3 5.3L3 13l3 3 4.2-4.2a4 4 0 0 0 5.3-5.3l-2.6 2.6-2.1-2.1 2.7-3.5Z" stroke-linejoin="round" />
            </g>
            <g v-else fill="none" stroke="currentColor" stroke-width="1.6">
              <circle cx="10" cy="10" r="7" />
              <path d="M10 6v4l2.8 1.6" stroke-linecap="round" />
            </g>
          </svg>
          <span>{{ item.label }}</span>
        </button>
      </nav>
      <div class="rail-foot" aria-hidden="true"><span class="rail-dot" /></div>
    </aside>

    <div class="stage">
      <header class="stage-head">
        <div>
          <h1>{{ tabMeta[tab].title }}</h1>
          <p class="stage-hint">{{ tabMeta[tab].hint }}</p>
        </div>
        <div class="stage-status" role="status">
          <span v-if="backendError" class="pill pill-bad">Backend offline</span>
          <span v-else-if="!backendReady" class="pill">Connecting…</span>
          <span v-else class="pill pill-ok">Local engine ready</span>
        </div>
      </header>

      <section v-if="backendError" class="alert" role="alert">
        <div>
          <strong>Backend unreachable.</strong>
          <span class="dim">{{ backendError }}</span>
          <span class="dim">Run the desktop shell (<code>pnpm tauri dev</code>), not plain <code>vite dev</code>.</span>
        </div>
        <button class="btn ghost" @click="refresh">Retry</button>
      </section>

      <section
        v-if="tab === 'convert' && !backendError"
        class="workbench"
        :class="{ dragover: dragActive }"
        @dragenter="onDragEnter"
        @dragleave="onDragLeave"
        @dragover="onDragOver"
        @drop="onDrop"
      >
        <div class="col queue">
          <div class="col-head">
            <h2>Files</h2>
            <div class="col-actions">
              <button class="btn" @click="addFiles">Add files</button>
              <button class="btn ghost" :disabled="files.length === 0" @click="files = []">Clear</button>
            </div>
          </div>
          <button class="btn ghost dir" @click="chooseOutputDir" :title="outputDir ?? 'Same folder as input'">
            {{ outputDir ? outputDir : "Output folder (optional)" }}
          </button>
          <div v-if="dragActive" class="drop-overlay" aria-hidden="true">
            <div class="drop-hint">Drop images / PDFs to add them</div>
          </div>
          <ul v-if="files.length" class="files">
            <li v-for="info in infos" :key="info.name" class="file-card">
              <div class="file-badge">{{ info.format }}</div>
              <div class="file-meta">
                <strong>{{ info.name }}</strong>
                <span class="dim tabular">
                  <template v-if="info.width">{{ info.width }}×{{ info.height }} · </template>
                  {{ (info.size_bytes / 1024).toFixed(1) }} KB
                </span>
              </div>
              <button
                class="icon-btn"
                :aria-label="`Remove ${info.name}`"
                @click="removeFile(files[infos.indexOf(info)] ?? info.name)"
              >
                ×
              </button>
            </li>
          </ul>
          <div v-else class="empty">
            <p>No files yet — pick PNG, JPEG, WebP, BMP, TIFF or PDF files to convert.</p>
          </div>
        </div>

        <div class="col opts">
          <div class="col-head"><h2>Options</h2></div>
          <div class="card">
            <label class="field">
              <span>Format</span>
              <select v-model="target">
                <option v-for="f in encodable" :key="f.id" :value="f.id">{{ f.name }}</option>
              </select>
            </label>
            <label class="field">
              <span>Quality · {{ quality }}</span>
              <input v-model.number="quality" type="range" min="1" max="100" :disabled="webpLossless" />
            </label>
            <label v-if="target === 'png'" class="field">
              <span>PNG level · {{ pngLevel }}</span>
              <input v-model.number="pngLevel" type="range" min="0" max="9" />
            </label>
            <label v-if="target === 'webp'" class="check">
              <input v-model="webpLossless" type="checkbox" /> Lossless
            </label>
            <label class="check">
              <input v-model="stripMetadata" type="checkbox" /> Strip metadata
            </label>
          </div>

          <div class="card">
            <label class="field">
              <span>Resize</span>
              <select v-model="resizeMode">
                <option value="off">Keep size</option>
                <option value="exact">Exact W×H</option>
                <option value="fit">Fit inside</option>
                <option value="fill">Fill + crop</option>
              </select>
            </label>
            <label v-if="resizeMode === 'exact'" class="field inline">
              <span>Width × Height</span>
              <span class="pair">
                <input v-model.number="exactW" type="number" min="1" max="16384" />
                <span>×</span>
                <input v-model.number="exactH" type="number" min="1" max="16384" />
              </span>
            </label>
            <label v-if="resizeMode === 'fit'" class="field">
              <span>Fit box (W×H)</span>
              <input v-model="fitBox" type="text" placeholder="800x600" pattern="\d+[xX]\d+" />
            </label>
            <label v-if="resizeMode === 'fill'" class="field">
              <span>Fill box (W×H)</span>
              <input v-model="fillBox" type="text" placeholder="800x600" pattern="\d+[xX]\d+" />
            </label>
            <label v-if="resizeMode !== 'off'" class="field">
              <span>Filter</span>
              <select v-model="filter">
                <option value="lanczos3">Lanczos3</option>
                <option value="catmullrom">CatmullRom</option>
                <option value="gaussian">Gaussian</option>
                <option value="nearest">Nearest</option>
              </select>
            </label>
            <label v-if="resizeMode === 'fit' || resizeMode === 'fill'" class="check">
              <input v-model="upscale" type="checkbox" /> Upscale
            </label>
          </div>

          <div class="presets">
            <span class="dim">Presets</span>
            <button
              v-for="p in presets"
              :key="p.key"
              class="chip"
              @click="applyPreset(p.key)"
              :title="`${p.name} → ${p.format.toUpperCase()} q${p.quality}`"
            >
              {{ p.name }}
            </button>
          </div>
        </div>

        <div class="col action">
          <div class="action-stick">
            <button class="primary grow" :disabled="!canConvert" @click="convert">
              <span v-if="busy" class="spinner" aria-hidden="true" />{{ busy ? "Converting…" : `Convert${files.length ? ` ${files.length}` : ""}` }}
            </button>
            <button v-if="busy" class="btn danger" @click="cancelConvert" :disabled="cancelling">
              {{ cancelling ? "Cancelling…" : "Cancel" }}
            </button>
            <div v-if="busy" class="progress-wrap" role="status">
              <div class="progress-bar"><div class="progress-fill" :style="{ width: `${Math.round(progressFraction * 100)}%` }" /></div>
              <p class="progress">{{ progress }}</p>
            </div>
            <p v-if="error" class="error" role="alert">{{ error }}</p>
            <div v-if="result" class="result card">
              <p class="result-head">
                {{ result.outputs.length }} written<span v-if="result.skipped.length">, {{ result.skipped.length }} skipped</span>
              </p>
              <ul class="out-list">
                <li v-for="o in result.outputs" :key="o" class="out-row">
                  <button class="link" @click="reveal(o)" :title="`Reveal ${fileName(o)} in folder`">
                    {{ fileName(o) }}
                  </button>
                  <span v-if="sizeFor(o)" class="dim savings tabular">{{ savingsText(sizeFor(o)!.input_bytes, sizeFor(o)!.output_bytes) }}</span>
                  <button class="link dim folder-link" @click="reveal(o)" :title="`Open ${parentDir(o)}`">
                    {{ parentDir(o) }}
                  </button>
                </li>
                <li v-for="s in result.skipped" :key="s" class="out-row skipped">
                  <span>{{ fileName(s) }} <span class="dim">(already exists)</span></span>
                </li>
              </ul>
              <ul v-if="result.failures.length" class="failures">
                <li v-for="f in result.failures" :key="f" class="error">{{ f }}</li>
              </ul>
              <p v-if="revealFailed" class="error">{{ revealFailed }}</p>
            </div>
            <div class="card favicon">
              <button class="btn" @click="runFavicon">Favicon set (ico + PNGs)</button>
              <p class="dim">favicon.ico (16/32/48) + icon PNGs + HTML snippet from the first image.</p>
              <pre v-if="faviconSnippet" class="code-snippet">{{ faviconSnippet }}</pre>
            </div>
          </div>
        </div>
      </section>

      <section v-if="tab === 'pdf' && !backendError" class="workbench narrow">
        <div class="col queue">
          <div class="col-head"><h2>Source</h2></div>
          <div class="drop-row">
            <button class="btn" @click="pickPdf">Choose PDF</button>
            <button class="btn ghost" :disabled="!pdfFile" @click="pdfFile = null; pdfPages = null; pdfResult = null">Clear</button>
          </div>
          <div v-if="pdfFile" class="card file-card">
            <div class="file-badge">PDF</div>
            <div class="file-meta">
              <strong>{{ fileName(pdfFile) }}</strong>
              <span class="dim">{{ pdfPages !== null ? `${pdfPages} pages` : "page count unavailable" }} · {{ pdfFile }}</span>
            </div>
          </div>
          <div v-else class="empty">
            <p>Pick a PDF to split pages or export to Word (.docx).</p>
          </div>
          <div class="card">
            <div class="drop-row">
              <button class="btn" @click="pickMergeFiles">Merge PDFs ({{ mergeFiles.length }})</button>
              <button class="btn ghost" :disabled="mergeFiles.length === 0" @click="mergeFiles = []">Clear</button>
            </div>
            <ul v-if="mergeFiles.length" class="files">
              <li v-for="(f, i) in mergeFiles" :key="f" class="file-card">
                <div class="file-badge">{{ i + 1 }}</div>
                <div class="file-meta"><strong>{{ fileName(f) }}</strong></div>
              </li>
            </ul>
            <button v-if="mergeFiles.length" class="primary grow" :disabled="pdfBusy" @click="runPdfMerge">
              {{ pdfBusy ? "Merging…" : `Merge ${mergeFiles.length} PDFs` }}
            </button>
          </div>
        </div>
        <div class="col opts">
          <div class="col-head"><h2>Operation</h2></div>
          <div v-if="pdfFile" class="card">
            <label class="field grow">
              <span>Pages (e.g. 1-3, 1,3,5-7; blank = all for Word)</span>
              <input v-model="pdfRange" type="text" placeholder="1-2" pattern="[\d\s,\-]+" />
            </label>
          </div>
          <div v-if="pdfFile" class="action-row">
            <button class="primary grow" :disabled="pdfBusy || !pdfRange.trim()" @click="runPdfSplit">
              {{ pdfBusy ? "Splitting…" : "Split PDF" }}
            </button>
            <button class="btn grow" :disabled="pdfBusy" @click="runPdfToDocx">
              {{ pdfBusy ? "Exporting…" : "Export .docx" }}
            </button>
          </div>
          <div v-if="pdfFile" class="card">
            <label class="field">
              <span>Compress</span>
              <select v-model="pdfCompressLevel">
                <option value="light">Light (prune only)</option>
                <option value="balanced">Balanced (prune + recompress)</option>
              </select>
            </label>
            <button class="btn grow" :disabled="pdfBusy" @click="runPdfCompress">
              {{ pdfBusy ? "Compressing…" : "Compress PDF" }}
            </button>
          </div>
          <div v-if="pdfFile" class="card">
            <label class="field">
              <span>Render DPI</span>
              <input v-model.number="pdfRenderDpi" type="number" min="1" max="1200" />
            </label>
            <label class="field">
              <span>Format</span>
              <select v-model="pdfRenderFormat">
                <option value="png">PNG</option>
                <option value="jpg">JPEG</option>
                <option value="webp">WebP</option>
              </select>
            </label>
            <button class="btn grow" :disabled="pdfBusy" @click="runPdfRender">
              {{ pdfBusy ? "Rendering…" : "Render pages" }}
            </button>
          </div>
          <div v-if="pdfResult" class="result card">
            <p class="result-head">Written</p>
            <button class="link" @click="reveal(pdfResult)" :title="`Reveal ${fileName(pdfResult)} in folder`">
              {{ fileName(pdfResult) }}
            </button>
          </div>
        </div>
      </section>

      <section v-if="tab === 'diagram'" class="diagram-wrap">
        <DiagramTab />
      </section>

      <section v-if="tab === 'tools' && !backendError" class="panel-soft">
        <ToolsTab />
      </section>

      <section v-if="tab === 'history' && !backendError" class="narrow-list">
        <div class="col-head">
          <h2>Recent jobs</h2>
          <button class="btn ghost" @click="refresh">Refresh</button>
        </div>
        <ul v-if="history.length" class="history">
          <li v-for="h in history" :key="h.job_id" class="hist-row">
            <span class="op">{{ h.operation }}</span>
            <span class="dim">{{ h.input }} → {{ h.output }}</span>
            <span class="status">{{ h.status }}</span>
          </li>
        </ul>
        <div v-else class="empty">
          <p>No history yet — convert something first.</p>
        </div>
      </section>
    </div>
  </main>
</template>

<style>
:root {
  color-scheme: dark;
  --bg: #12100e;
  --bg-raise: #1a1714;
  --panel: #1c1917;
  --panel-soft: #171412;
  --ink: #f3ede3;
  --ink-dim: #cfc6b8;
  --dim: #a39e93;
  --line: rgb(255 255 255 / 0.08);
  --line-soft: rgb(255 255 255 / 0.05);
  --brand: #e86a2c;
  --brand-deep: #c8521a;
  --brand-ink: #1c1008;
  --ok: #4ade80;
  --ok-dim: #14532d;
  --danger: #f87171;
  --radius: 12px;
  --shadow: 0 12px 32px -12px rgb(0 0 0 / 0.55);
}
* {
  box-sizing: border-box;
}
::selection {
  background: rgb(232 106 44 / 0.35);
  color: var(--ink);
}
body {
  margin: 0;
  background: var(--bg);
  color: var(--ink);
  font-family: "Segoe UI", system-ui, -apple-system, sans-serif;
}
::-webkit-scrollbar {
  width: 10px;
  height: 10px;
}
::-webkit-scrollbar-thumb {
  background: #3a342e;
  border-radius: 8px;
  border: 2px solid var(--bg);
}
::-webkit-scrollbar-track {
  background: transparent;
}
:focus-visible {
  outline: 2px solid var(--brand);
  outline-offset: 2px;
  border-radius: 6px;
}
button,
input,
select,
textarea {
  font: inherit;
  color: inherit;
}
input,
select,
textarea {
  caret-color: var(--brand);
}
.tabular {
  font-variant-numeric: tabular-nums;
}
</style>

<style scoped>
.app {
  display: flex;
  min-height: 100vh;
  background: var(--bg);
}
.rail {
  width: 196px;
  flex: none;
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.1rem 0.8rem;
  border-right: 1px solid var(--line-soft);
  background: var(--bg-raise);
  position: sticky;
  top: 0;
  height: 100vh;
}
.rail-brand {
  display: flex;
  align-items: center;
  padding: 0 0.4rem;
}
.rail-mark {
  width: 34px;
  height: 34px;
  border-radius: 10px;
  display: grid;
  place-items: center;
  font-weight: 800;
  font-size: 1.05rem;
  color: #fff;
  background: linear-gradient(135deg, var(--brand), var(--brand-deep));
}
.rail-nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.rail-nav button {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  border: 0;
  border-radius: 10px;
  background: transparent;
  color: var(--dim);
  font-weight: 600;
  font-size: 0.92rem;
  padding: 0.55rem 0.65rem;
  cursor: pointer;
  text-align: left;
}
.rail-nav button:hover {
  color: var(--ink);
  background: rgb(255 255 255 / 0.04);
}
.rail-nav button.active {
  color: var(--ink);
  background: rgb(232 106 44 / 0.14);
}
.rail-nav button.active .rail-icon {
  color: var(--brand);
}
.rail-icon {
  width: 20px;
  height: 20px;
  flex: none;
}
.rail-foot {
  margin-top: auto;
  padding: 0 0.4rem;
}
.rail-dot {
  display: block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--ok);
}
.stage {
  flex: 1;
  min-width: 0;
  padding: 1.4rem clamp(1rem, 3vw, 2.2rem) 3rem;
  display: flex;
  flex-direction: column;
  gap: 1.1rem;
}
.stage-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-end;
  gap: 1rem;
  flex-wrap: wrap;
}
.stage-head h1 {
  margin: 0;
  font-size: 1.45rem;
  letter-spacing: -0.02em;
}
.stage-hint {
  margin: 0.2rem 0 0;
  color: var(--dim);
  font-size: 0.88rem;
}
.pill {
  border: 1px solid var(--line);
  border-radius: 999px;
  padding: 0.3rem 0.75rem;
  font-size: 0.78rem;
  font-weight: 600;
  color: var(--dim);
}
.pill-ok {
  color: var(--ok);
  border-color: rgb(74 222 128 / 0.35);
  background: rgb(74 222 128 / 0.08);
}
.pill-bad {
  color: var(--danger);
  border-color: rgb(248 113 113 / 0.4);
  background: rgb(248 113 113 / 0.08);
}
.alert {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1rem;
  border: 1px solid rgb(248 113 113 / 0.35);
  border-radius: var(--radius);
  background: rgb(248 113 113 / 0.07);
  padding: 0.9rem 1rem;
}
.workbench {
  display: grid;
  grid-template-columns: minmax(260px, 1.1fr) minmax(250px, 0.9fr) minmax(280px, 1fr);
  gap: 1rem;
  align-items: start;
}
.workbench.narrow {
  grid-template-columns: minmax(280px, 1fr) minmax(280px, 1fr);
  max-width: 1100px;
}
.col {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  min-width: 0;
}
.col-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 0.6rem;
}
.col-head h2 {
  margin: 0;
  font-size: 0.8rem;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--dim);
}
.col-actions {
  display: flex;
  gap: 0.4rem;
}
.card {
  border: 1px solid var(--line-soft);
  border-radius: var(--radius);
  background: var(--panel);
  padding: 0.95rem;
  display: flex;
  flex-direction: column;
  gap: 0.7rem;
}
.panel-soft {
  border: 1px solid var(--line-soft);
  border-radius: var(--radius);
  background: var(--panel-soft);
  padding: 1rem;
}
.narrow-list {
  max-width: 860px;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}
.diagram-wrap {
  width: 100%;
}
.btn {
  padding: 0.5rem 0.9rem;
  border-radius: 8px;
  border: 1px solid var(--line);
  background: #2a2521;
  color: var(--ink);
  font-weight: 600;
  font-size: 0.88rem;
  cursor: pointer;
}
.btn:hover:not(:disabled) {
  border-color: rgb(255 255 255 / 0.18);
}
.btn:disabled {
  opacity: 0.45;
  cursor: default;
}
.btn.ghost {
  background: transparent;
}
.btn.danger {
  background: transparent;
  color: var(--danger);
  border-color: rgb(248 113 113 / 0.5);
}
.btn.dir {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
  font-weight: 500;
  color: var(--ink-dim);
}
.primary {
  border: 0;
  border-radius: 10px;
  padding: 0.8rem;
  font-size: 1rem;
  font-weight: 700;
  color: #fff;
  background: linear-gradient(135deg, var(--brand), var(--brand-deep));
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.6rem;
}
.primary:disabled {
  opacity: 0.5;
  cursor: default;
}
.grow {
  flex: 1;
}
.action {
  position: sticky;
  top: 1rem;
}
.action-stick {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  position: sticky;
  top: 1rem;
}
.action-row {
  display: flex;
  gap: 0.5rem;
}
.convert-row {
  display: flex;
  gap: 0.5rem;
}
.drop-row {
  display: flex;
  gap: 0.5rem;
  flex-wrap: wrap;
  align-items: center;
}
.files {
  list-style: none;
  padding: 0;
  margin: 0;
  display: grid;
  gap: 0.45rem;
  max-height: 46vh;
  overflow-y: auto;
}
.file-card {
  display: flex;
  gap: 0.7rem;
  align-items: center;
  border: 1px solid var(--line-soft);
  border-radius: 10px;
  padding: 0.55rem 0.65rem;
  background: var(--panel);
}
.file-badge {
  font-size: 0.7rem;
  font-weight: 800;
  letter-spacing: 0.04em;
  background: rgb(232 106 44 / 0.14);
  color: var(--brand);
  border-radius: 7px;
  padding: 0.28rem 0.5rem;
  white-space: nowrap;
}
.file-meta {
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
  min-width: 0;
}
.file-meta strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.88rem;
}
.dim {
  color: var(--dim);
  font-size: 0.85rem;
}
code {
  font-family: ui-monospace, monospace;
  font-size: 0.82rem;
}
.icon-btn {
  margin-left: auto;
  border: 0;
  background: transparent;
  color: var(--dim);
  font-size: 1.05rem;
  line-height: 1;
  cursor: pointer;
  border-radius: 8px;
  padding: 0.3rem 0.5rem;
  flex: none;
}
.icon-btn:hover {
  color: var(--ink);
  background: rgb(255 255 255 / 0.06);
}
.empty {
  border: 1px dashed var(--line);
  border-radius: var(--radius);
  padding: 1.6rem 1rem;
  text-align: center;
  color: var(--dim);
  background: transparent;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  font-size: 0.85rem;
}
.field > span {
  color: var(--dim);
  font-weight: 600;
  font-size: 0.76rem;
  letter-spacing: 0.03em;
}
.field input[type="text"],
.field input[type="number"],
.field select {
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 0.45rem 0.6rem;
  background: var(--bg-raise);
  color: var(--ink);
  font-size: 0.9rem;
}
.field input[type="range"] {
  accent-color: var(--brand);
  width: 100%;
}
.field input[type="number"] {
  width: 5.5rem;
}
.field.grow {
  flex: 1;
  min-width: 180px;
}
.pair {
  display: inline-flex;
  gap: 0.35rem;
  align-items: center;
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 0.3rem 0.5rem;
  background: var(--bg-raise);
}
.pair input {
  width: 4.5rem;
  border: 0;
  background: transparent;
  padding: 0.15rem;
}
.check {
  display: flex;
  gap: 0.45rem;
  align-items: center;
  font-size: 0.88rem;
}
.check input {
  accent-color: var(--brand);
  width: 1rem;
  height: 1rem;
}
.presets {
  display: flex;
  gap: 0.4rem;
  align-items: center;
  flex-wrap: wrap;
}
.chip {
  border: 1px solid var(--line);
  background: transparent;
  color: var(--ink-dim);
  border-radius: 999px;
  padding: 0.32rem 0.75rem;
  font-size: 0.82rem;
  cursor: pointer;
}
.chip:hover {
  border-color: var(--brand);
  color: var(--brand);
}
.progress-wrap {
  margin-top: 0;
}
.progress-bar {
  height: 6px;
  border-radius: 999px;
  background: rgb(255 255 255 / 0.08);
  overflow: hidden;
}
.progress-fill {
  height: 100%;
  border-radius: 999px;
  background: linear-gradient(90deg, var(--brand), var(--brand-deep));
  transition: width 0.25s ease-out;
}
.workbench.dragover {
  outline: 2px dashed var(--brand);
  outline-offset: 6px;
  border-radius: var(--radius);
}
.drop-overlay {
  border: 1px dashed var(--brand);
  border-radius: var(--radius);
  background: rgb(232 106 44 / 0.08);
  padding: 1.1rem;
  text-align: center;
}
.drop-hint {
  font-weight: 700;
  color: var(--brand);
}
.savings {
  font-size: 0.8rem;
}
.spinner {
  width: 1rem;
  height: 1rem;
  border-radius: 50%;
  border: 2px solid rgb(255 255 255 / 0.35);
  border-top-color: white;
  animation: spin 0.7s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
.progress {
  color: var(--dim);
  font-size: 0.88rem;
  margin: 0.3rem 0 0;
}
.error {
  color: var(--danger);
  font-size: 0.88rem;
}
.result-head {
  margin: 0;
  font-weight: 700;
  color: var(--ok);
  font-size: 0.9rem;
}
.out-list,
.failures {
  list-style: none;
  padding: 0;
  margin: 0;
  display: grid;
  gap: 0.35rem;
}
.out-row {
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
  border-top: 1px solid var(--line-soft);
  padding-top: 0.45rem;
}
.out-row:first-child {
  border-top: 0;
  padding-top: 0;
}
.link {
  border: 0;
  background: none;
  padding: 0;
  cursor: pointer;
  text-align: left;
  font-size: 0.9rem;
  font-weight: 700;
  color: var(--brand);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.link:hover {
  text-decoration: underline;
}
.folder-link {
  font-weight: 400;
  font-size: 0.8rem;
}
.out-row.skipped {
  color: var(--dim);
}
.code-snippet {
  white-space: pre-wrap;
  word-break: break-all;
  font-size: 0.76rem;
  margin: 0;
  color: var(--ink-dim);
}
.favicon .dim {
  margin: 0;
  font-size: 0.82rem;
}
.history {
  list-style: none;
  padding: 0;
  margin: 0;
  display: grid;
  gap: 0.4rem;
}
.hist-row {
  display: flex;
  gap: 0.6rem;
  align-items: baseline;
  border: 1px solid var(--line-soft);
  border-radius: 10px;
  padding: 0.55rem 0.7rem;
  background: var(--panel);
}
.op {
  font-weight: 700;
  text-transform: capitalize;
}
.status {
  margin-left: auto;
  font-size: 0.76rem;
  font-weight: 700;
  background: rgb(74 222 128 / 0.12);
  color: var(--ok);
  border-radius: 999px;
  padding: 0.15rem 0.6rem;
  white-space: nowrap;
}
@media (max-width: 1100px) {
  .workbench {
    grid-template-columns: minmax(240px, 1fr) minmax(240px, 1fr);
  }
  .workbench .action {
    grid-column: 1 / -1;
  }
  .action-stick {
    position: static;
  }
}
@media (max-width: 760px) {
  .app {
    flex-direction: column;
  }
  .rail {
    width: 100%;
    height: auto;
    position: static;
    flex-direction: row;
    align-items: center;
    border-right: 0;
    border-bottom: 1px solid var(--line-soft);
    padding: 0.6rem 0.8rem;
  }
  .rail-nav {
    flex-direction: row;
    overflow-x: auto;
  }
  .rail-foot {
    display: none;
  }
  .workbench,
  .workbench.narrow {
    grid-template-columns: 1fr;
  }
}
</style>
