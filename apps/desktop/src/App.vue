<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import {
  api,
  isCommandError,
  pickDirectory,
  pickFiles,
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
const outputDir = ref<string | null>(null);
const busy = ref(false);
const result = ref<ConvertDone | null>(null);
const error = ref<string | null>(null);
const history = ref<HistoryRow[]>([]);
const tab = ref<"convert" | "history">("convert");

const encodable = computed(() => formats.value.filter((f) => f.can_encode));
const canConvert = computed(
  () => !busy.value && files.value.length > 0 && target.value.length > 0,
);

async function refresh() {
  formats.value = await api.formats();
  presets.value = await api.presets();
  history.value = await api.history(20);
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
    } catch (err) {
      infos.value.push({
        name: path.split(/[/\\]/).pop() ?? path,
        format: "Unknown",
        mime_type: "",
        width: null,
        height: null,
        pixel: null,
        alpha: false,
        size_bytes: 0,
      });
      void err;
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
  try {
    result.value = await api.convert({
      inputs: files.value,
      to: target.value,
      outputDir: outputDir.value ?? undefined,
      quality: quality.value,
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
    <header>
      <h1>ForgeConvert</h1>
      <p class="sub">Local-first image &amp; PDF conversion. Files never leave your machine.</p>
      <nav>
        <button :class="{ active: tab === 'convert' }" @click="tab = 'convert'">Convert</button>
        <button :class="{ active: tab === 'history' }" @click="tab = 'history'">History</button>
      </nav>
    </header>

    <section v-if="tab === 'convert'">
      <div class="row">
        <button @click="addFiles">Choose files</button>
        <button :disabled="files.length === 0" @click="files = []">Clear</button>
        <button @click="chooseOutputDir">
          {{ outputDir ? `Output: ${outputDir}` : "Output folder (optional)" }}
        </button>
      </div>

      <ul v-if="files.length" class="files">
        <li v-for="info in infos" :key="info.name">
          <strong>{{ info.name }}</strong>
          <span>{{ info.format }}</span>
          <span v-if="info.width">{{ info.width }}×{{ info.height }}</span>
          <span>{{ (info.size_bytes / 1024).toFixed(1) }} KB</span>
          <button @click="removeFile(files[infos.indexOf(info)] ?? info.name)">✕</button>
        </li>
      </ul>
      <p v-else class="hint">Drop files via the picker — drag &amp; drop lands in a later iteration.</p>

      <div class="controls">
        <label>
          Format
          <select v-model="target">
            <option v-for="f in encodable" :key="f.id" :value="f.id">
              {{ f.name }}
            </option>
          </select>
        </label>
        <label>
          Quality {{ quality }}
          <input v-model.number="quality" type="range" min="1" max="100" />
        </label>
        <label class="check">
          <input v-model="stripMetadata" type="checkbox" /> Strip metadata
        </label>
      </div>

      <div class="row">
        <span>Presets:</span>
        <button v-for="p in presets" :key="p.key" @click="applyPreset(p.key)">
          {{ p.name }}
        </button>
      </div>

      <button class="primary" :disabled="!canConvert" @click="convert">
        {{ busy ? "Converting…" : "Convert" }}
      </button>

      <p v-if="error" class="error">{{ error }}</p>
      <div v-if="result" class="result">
        <p>✅ {{ result.outputs.length }} written, {{ result.skipped.length }} skipped</p>
        <ul>
          <li v-for="o in result.outputs" :key="o">{{ o }}</li>
        </ul>
        <ul v-if="result.failures.length">
          <li v-for="f in result.failures" :key="f" class="error">{{ f }}</li>
        </ul>
      </div>
    </section>

    <section v-else>
      <button @click="refresh">Refresh</button>
      <ul class="history">
        <li v-for="h in history" :key="h.job_id">
          <strong>{{ h.operation }}</strong> {{ h.input }} → {{ h.output }}
          <span>{{ h.status }}</span>
        </li>
      </ul>
      <p v-if="!history.length" class="hint">No history yet — convert something first.</p>
    </section>
  </main>
</template>

<style scoped>
.app {
  max-width: 860px;
  margin: 0 auto;
  padding: 1.5rem;
  font-family: system-ui, sans-serif;
}
header h1 {
  margin: 0;
}
.sub {
  color: #666;
  margin: 0.25rem 0 1rem;
}
nav {
  display: flex;
  gap: 0.5rem;
  margin-bottom: 1rem;
}
button {
  padding: 0.45rem 0.9rem;
  border: 1px solid #ccc;
  border-radius: 6px;
  background: #f7f7f7;
  cursor: pointer;
}
button:disabled {
  opacity: 0.5;
  cursor: default;
}
button.active,
button.primary {
  background: #e86a2c;
  border-color: #e86a2c;
  color: white;
}
button.primary {
  margin-top: 1rem;
  font-size: 1.05rem;
}
.row {
  display: flex;
  gap: 0.5rem;
  align-items: center;
  flex-wrap: wrap;
  margin: 0.75rem 0;
}
.files,
.history {
  list-style: none;
  padding: 0;
}
.files li,
.history li {
  display: flex;
  gap: 0.75rem;
  align-items: center;
  padding: 0.4rem 0;
  border-bottom: 1px solid #eee;
}
.controls {
  display: flex;
  gap: 1rem;
  align-items: center;
  flex-wrap: wrap;
  margin: 0.75rem 0;
}
.hint {
  color: #888;
}
.error {
  color: #b3261e;
}
.result {
  margin-top: 1rem;
}
</style>
