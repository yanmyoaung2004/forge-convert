<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
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
const progress = ref<string | null>(null);
const result = ref<ConvertDone | null>(null);
const error = ref<string | null>(null);
const backendError = ref<string | null>(null);
const history = ref<HistoryRow[]>([]);
const tab = ref<"convert" | "history">("convert");
const revealFailed = ref<string | null>(null);

const encodable = computed(() => formats.value.filter((f) => f.can_encode));
const backendReady = computed(() => formats.value.length > 0);
const canConvert = computed(
  () => backendReady.value && !busy.value && files.value.length > 0 && target.value.length > 0,
);

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

async function convert() {
  if (!canConvert.value) return;
  busy.value = true;
  error.value = null;
  result.value = null;
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
      stripMetadata: stripMetadata.value,
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

function applyPreset(key: string) {
  const preset = presets.value.find((p) => p.key === key);
  if (!preset) return;
  target.value = preset.format === "jpg" ? "jpg" : preset.format;
  quality.value = preset.quality;
}

onMounted(() => void refresh());
</script>

<template>
  <main class="app">
    <header class="hero">
      <div class="brand">
        <div class="logo">Fc</div>
        <div>
          <h1>ForgeConvert</h1>
          <p class="sub">Local-first image &amp; PDF conversion. Files never leave your machine.</p>
        </div>
      </div>
      <nav class="tabs" role="tablist">
        <button :class="{ active: tab === 'convert' }" role="tab" @click="tab = 'convert'">Convert</button>
        <button :class="{ active: tab === 'history' }" role="tab" @click="tab = 'history'">History</button>
      </nav>
    </header>

    <section v-if="backendError" class="panel banner" role="alert">
      <strong>Backend unreachable.</strong>
      <span>{{ backendError }}</span>
      <span class="dim">Run the desktop shell (<code>pnpm tauri dev</code>), not plain <code>vite dev</code> — conversion commands live in Rust.</span>
      <button class="btn ghost" @click="refresh">Retry</button>
    </section>

    <section v-if="tab === 'convert' && !backendError" class="panel">
      <div class="drop-row">
        <button class="btn" @click="addFiles">＋ Choose files</button>
        <button class="btn ghost" :disabled="files.length === 0" @click="files = []">Clear</button>
        <button class="btn ghost dir" @click="chooseOutputDir" :title="outputDir ?? 'Same folder as input'">
          {{ outputDir ? `📁 ${outputDir}` : "📁 Output folder (optional)" }}
        </button>
      </div>

      <ul v-if="files.length" class="files">
        <li v-for="info in infos" :key="info.name" class="file-card">
          <div class="file-badge">{{ info.format }}</div>
          <div class="file-meta">
            <strong>{{ info.name }}</strong>
            <span class="dim">
              <template v-if="info.width">{{ info.width }}×{{ info.height }} · </template>
              {{ (info.size_bytes / 1024).toFixed(1) }} KB
            </span>
          </div>
          <button
            class="icon-btn"
            :aria-label="`Remove ${info.name}`"
            @click="removeFile(files[infos.indexOf(info)] ?? info.name)"
          >
            ✕
          </button>
        </li>
      </ul>
      <div v-else class="empty">
        <div class="empty-icon">🖼️</div>
        <p>No files yet — pick PNG, JPEG, WebP, BMP, TIFF or PDF files to convert.</p>
      </div>

      <div class="controls card">
        <label class="field">
          <span>Format</span>
          <select v-model="target">
            <option v-for="f in encodable" :key="f.id" :value="f.id">
              {{ f.name }}
            </option>
          </select>
        </label>
        <label class="field grow">
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

      <div class="controls card">
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

      <button class="primary" :disabled="!canConvert" @click="convert">
        <span v-if="busy" class="spinner" aria-hidden="true" />{{ busy ? "Converting…" : `Convert ${files.length || ""}`.trim() }}
      </button>
      <p v-if="progress" class="progress" role="status">{{ progress }}</p>

      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <div v-if="result" class="result card">
        <p class="result-head">
          ✅ {{ result.outputs.length }} written<span v-if="result.skipped.length">, {{ result.skipped.length }} skipped</span>
        </p>
        <ul class="out-list">
          <li v-for="o in result.outputs" :key="o" class="out-row">
            <button class="link" @click="reveal(o)" :title="`Reveal ${fileName(o)} in folder`">
              📄 {{ fileName(o) }}
            </button>
            <button class="link dim folder-link" @click="reveal(o)" :title="`Open ${parentDir(o)}`">
              {{ parentDir(o) }} ⧉
            </button>
          </li>
          <li v-for="s in result.skipped" :key="s" class="out-row skipped">
            <span>⏭ {{ fileName(s) }} <span class="dim">(already exists)</span></span>
          </li>
        </ul>
        <ul v-if="result.failures.length" class="failures">
          <li v-for="f in result.failures" :key="f" class="error">{{ f }}</li>
        </ul>
        <p v-if="revealFailed" class="error">{{ revealFailed }}</p>
      </div>
    </section>

    <section v-else-if="!backendError" class="panel">
      <div class="drop-row">
        <button class="btn ghost" @click="refresh">↻ Refresh</button>
      </div>
      <ul v-if="history.length" class="history">
        <li v-for="h in history" :key="h.job_id" class="hist-row">
          <span class="op">{{ h.operation }}</span>
          <span class="dim">{{ h.input }} → {{ h.output }}</span>
          <span class="status">{{ h.status }}</span>
        </li>
      </ul>
      <div v-else class="empty">
        <div class="empty-icon">📜</div>
        <p>No history yet — convert something first.</p>
      </div>
    </section>
  </main>
</template>

<style>
:root {
  color-scheme: light;
  --bg: #f6f4ef;
  --panel: #ffffff;
  --ink: #1c1917;
  --dim: #78716c;
  --line: #e7e2d9;
  --brand: #e86a2c;
  --brand-deep: #c8521a;
  --brand-soft: #fdeede;
  --ok: #15803d;
  --danger: #b3261e;
  --radius: 14px;
  --shadow: 0 1px 2px rgb(28 25 23 / 0.06), 0 8px 24px -12px rgb(28 25 23 / 0.18);
}
* {
  box-sizing: border-box;
}
body {
  margin: 0;
  background:
    radial-gradient(1200px 400px at 20% -10%, #fdeede 0%, transparent 60%),
    radial-gradient(1000px 380px at 90% -10%, #e8f0fe 0%, transparent 55%),
    var(--bg);
  color: var(--ink);
  font-family: "Segoe UI", system-ui, -apple-system, sans-serif;
}
</style>

<style scoped>
.app {
  max-width: 920px;
  margin: 0 auto;
  padding: 2rem 1.5rem 3rem;
}
.hero {
  display: flex;
  justify-content: space-between;
  align-items: flex-end;
  gap: 1rem;
  flex-wrap: wrap;
  margin-bottom: 1.25rem;
}
.brand {
  display: flex;
  gap: 0.9rem;
  align-items: center;
}
.logo {
  width: 52px;
  height: 52px;
  border-radius: 16px;
  display: grid;
  place-items: center;
  font-weight: 800;
  font-size: 1.3rem;
  color: white;
  background: linear-gradient(135deg, var(--brand), var(--brand-deep));
  box-shadow: var(--shadow);
}
header h1 {
  margin: 0;
  font-size: 1.7rem;
  letter-spacing: -0.02em;
}
.sub {
  color: var(--dim);
  margin: 0.15rem 0 0;
  font-size: 0.92rem;
}
.tabs {
  display: flex;
  gap: 0.4rem;
  background: rgb(255 255 255 / 0.7);
  border: 1px solid var(--line);
  border-radius: 999px;
  padding: 0.25rem;
  backdrop-filter: blur(6px);
}
.tabs button {
  border: 0;
  background: transparent;
  border-radius: 999px;
  padding: 0.45rem 1.1rem;
  font-weight: 600;
  color: var(--dim);
  cursor: pointer;
}
.tabs button.active {
  background: var(--ink);
  color: white;
}
.panel {
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  box-shadow: var(--shadow);
  padding: 1.25rem;
}
.drop-row {
  display: flex;
  gap: 0.5rem;
  flex-wrap: wrap;
  align-items: center;
}
.btn {
  padding: 0.55rem 1rem;
  border-radius: 10px;
  border: 1px solid var(--line);
  background: var(--ink);
  color: white;
  font-weight: 600;
  cursor: pointer;
  transition: transform 0.06s ease, box-shadow 0.15s ease;
}
.btn:hover:not(:disabled) {
  transform: translateY(-1px);
}
.drop-row {
  display: flex;
  gap: 0.5rem;
  flex-wrap: wrap;
  align-items: center;
}
.banner {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  border-left: 4px solid var(--danger);
  margin-bottom: 1rem;
}
.banner code {
  background: #f5f0e8;
  border-radius: 6px;
  padding: 0.05rem 0.35rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.files {
  list-style: none;
  padding: 0;
  margin: 1rem 0 0;
  display: grid;
  gap: 0.5rem;
}
.file-card {
  display: flex;
  gap: 0.8rem;
  align-items: center;
  border: 1px solid var(--line);
  border-radius: 12px;
  padding: 0.6rem 0.7rem;
  background: #fffdf9;
}
.file-badge {
  font-size: 0.72rem;
  font-weight: 800;
  letter-spacing: 0.04em;
  background: var(--brand-soft);
  color: var(--brand-deep);
  border-radius: 8px;
  padding: 0.3rem 0.55rem;
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
}
.dim {
  color: var(--dim);
  font-size: 0.85rem;
}
.icon-btn {
  margin-left: auto;
  border: 0;
  background: transparent;
  color: var(--dim);
  font-size: 1rem;
  cursor: pointer;
  border-radius: 8px;
  padding: 0.3rem 0.5rem;
}
.icon-btn:hover {
  background: #f5f0e8;
  color: var(--ink);
}
.empty {
  margin-top: 1rem;
  border: 1.5px dashed var(--line);
  border-radius: var(--radius);
  padding: 2rem 1rem;
  text-align: center;
  color: var(--dim);
  background: rgb(255 255 255 / 0.6);
}
.empty-icon {
  font-size: 2rem;
}
.controls {
  margin-top: 1rem;
}
.card {
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background: #fffdf9;
  padding: 1rem;
}
.controls.card {
  display: flex;
  gap: 1.25rem;
  align-items: center;
  flex-wrap: wrap;
}
.error {
  color: var(--danger);
}
.result {
  margin-top: 1rem;
}
.field input[type="text"],
.field input[type="number"],
.field select {
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 0.45rem 0.6rem;
  background: white;
  font-size: 0.9rem;
}
.field input[type="number"] {
  width: 5.5rem;
}
.pair {
  display: inline-flex;
  gap: 0.35rem;
  align-items: center;
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 0.45rem 0.6rem;
  background: white;
}
.field.grow {
  flex: 1;
  min-width: 180px;
}
.check {
  display: flex;
  gap: 0.45rem;
  align-items: center;
  font-size: 0.9rem;
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
  margin-top: 0.9rem;
}
.chip {
  border: 1px solid var(--line);
  background: white;
  border-radius: 999px;
  padding: 0.35rem 0.8rem;
  font-size: 0.85rem;
  cursor: pointer;
}
.chip:hover {
  border-color: var(--brand);
  color: var(--brand-deep);
  background: var(--brand-soft);
}
.primary {
  margin-top: 1.1rem;
  width: 100%;
  border: 0;
  border-radius: 12px;
  padding: 0.85rem;
  font-size: 1.05rem;
  font-weight: 700;
  color: white;
  background: linear-gradient(135deg, var(--brand), var(--brand-deep));
  box-shadow: var(--shadow);
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
.spinner {
  width: 1rem;
  height: 1rem;
  border-radius: 50%;
  border: 2px solid rgb(255 255 255 / 0.4);
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
  font-size: 0.9rem;
}
.error {
  color: var(--danger);
}
.result {
  margin-top: 1rem;
}
.result-head {
  margin: 0 0 0.5rem;
  font-weight: 700;
  color: var(--ok);
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
  border-top: 1px solid var(--line);
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
  font-size: 0.95rem;
  font-weight: 700;
  color: var(--brand-deep);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.link:hover {
  text-decoration: underline;
}
.folder-link {
  font-weight: 400;
  font-size: 0.82rem;
}
.out-row.skipped {
  color: var(--dim);
}
.history {
  list-style: none;
  padding: 0;
  margin: 0.75rem 0 0;
  display: grid;
  gap: 0.4rem;
}
.hist-row {
  display: flex;
  gap: 0.6rem;
  align-items: baseline;
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 0.55rem 0.7rem;
}
.op {
  font-weight: 700;
  text-transform: capitalize;
}
.status {
  margin-left: auto;
  font-size: 0.8rem;
  background: #eef7ee;
  color: var(--ok);
  border-radius: 999px;
  padding: 0.15rem 0.6rem;
  white-space: nowrap;
}
</style>
