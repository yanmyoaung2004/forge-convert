<!-- SVG Diagram tab: owns doc + selection + history, wires canvas,
props, layout, templates, persistence, export. Backend only moves bytes. -->
<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import DiagramCanvas from "./DiagramCanvas.vue";
import PropsPanel from "./PropsPanel.vue";
import CodeViewer from "./CodeViewer.vue";
import { blankDoc, newId, validateDoc, type DiagramConnection, type DiagramDoc, type DiagramElement, type ElementKind, type Port } from "./types";
import { createElement, ELEMENTS } from "./elements";
import { History } from "./history";
import { layoutHierarchy, layoutHorizontal, layoutVertical } from "./layout";
import { toSvg } from "./serialize";
import { TEMPLATES } from "./templates";
import { isCommandError } from "../api";

const doc = ref<DiagramDoc>(blankDoc());
const selection = ref<string[]>([]);
const selConn = ref<string | null>(null);
const grid = ref(true);
const snap = ref(true);
const zoom = ref(1);
const error = ref<string | null>(null);
const filePath = ref<string | null>(null);
const dirty = ref(false);
const clipboard = ref<{ elements: DiagramElement[]; connections: DiagramConnection[] } | null>(null);
const history = new History(doc.value);
const editingLabel = ref<string | null>(null);

const selectedEl = computed(() => doc.value.elements.find((e) => e.id === selection.value[0]) ?? null);
const selectedConn = computed(() => doc.value.connections.find((c) => c.id === selConn.value) ?? null);
const svgText = computed(() => toSvg(doc.value));
const canUndo = ref(false);
const canRedo = ref(false);

function syncHistFlags(): void {
  canUndo.value = history.canUndo();
  canRedo.value = history.canRedo();
}

function touch(): void {
  doc.value.metadata.updatedAt = new Date().toISOString();
  dirty.value = true;
}

function commit(): void {
  history.commit(doc.value);
  syncHistFlags();
  touch();
}

function doUndo(): void {
  const d = history.undo();
  if (d) {
    doc.value = d;
    selection.value = [];
    selConn.value = null;
    syncHistFlags();
    touch();
  }
}

function doRedo(): void {
  const d = history.redo();
  if (d) {
    doc.value = d;
    selection.value = [];
    selConn.value = null;
    syncHistFlags();
    touch();
  }
}

function onSelect(ids: string[], additive: boolean): void {
  selConn.value = null;
  if (additive) {
    const s = new Set(selection.value);
    for (const id of ids) {
      if (s.has(id)) s.delete(id);
      else s.add(id);
    }
    selection.value = [...s];
  } else {
    selection.value = ids;
  }
}

function onMove(moves: Array<{ id: string; x: number; y: number }>, isCommit: boolean): void {
  for (const m of moves) {
    const el = doc.value.elements.find((e) => e.id === m.id);
    if (el) {
      el.x = m.x;
      el.y = m.y;
    }
  }
  if (isCommit) commit();
  else touch();
}

function onResize(id: string, x: number, y: number, w: number, h: number, isCommit: boolean): void {
  const el = doc.value.elements.find((e) => e.id === id);
  if (el) {
    el.x = x;
    el.y = y;
    el.w = w;
    el.h = h;
  }
  if (isCommit) commit();
  else touch();
}

function addElement(kind: ElementKind, x: number, y: number): void {
  const el = createElement(kind, x, y);
  doc.value.elements.push(el);
  selection.value = [el.id];
  selConn.value = null;
  commit();
}

/** Palette dragstart: set BOTH MIME types (WebView2 sometimes drops custom types). */
function onPalDrag(e: DragEvent, kind: ElementKind): void {
  e.dataTransfer?.setData("text/diagram-kind", kind);
  e.dataTransfer?.setData("text/plain", kind);
  if (e.dataTransfer) e.dataTransfer.effectAllowed = "copy";
}

/** Click fallback: add at viewport center (works even if DnD is blocked). */
function addAtCenter(kind: ElementKind): void {
  const vp = doc.value.viewport;
  addElement(kind, vp.x + 400, vp.y + 250);
}

function onConnect(source: { node: string; port: Port }, target: { node: string; port: Port }): void {
  doc.value.connections.push({
    id: newId("conn"),
    source,
    target,
    kind: "curved",
    color: "#1c1917",
    width: 2,
    dash: "solid",
    arrow: "end",
    opacity: 1,
  });
  commit();
}

function onPatchEl(patch: Partial<DiagramElement>): void {
  const el = doc.value.elements.find((e) => e.id === selection.value[0]);
  if (el) {
    Object.assign(el, patch);
    commit();
  }
}

function onPatchConn(patch: Partial<DiagramConnection>): void {
  const c = doc.value.connections.find((x) => x.id === selConn.value);
  if (c) {
    Object.assign(c, patch);
    commit();
  }
}

function selectedIds(): Set<string> {
  const s = new Set(selection.value);
  if (selConn.value) s.add(selConn.value);
  return s;
}

function deleteSelection(): void {
  const ids = selectedIds();
  if (ids.size === 0) return;
  doc.value.elements = doc.value.elements.filter((e) => !ids.has(e.id));
  doc.value.connections = doc.value.connections.filter(
    (c) => !ids.has(c.id) && !ids.has(c.source.node) && !ids.has(c.target.node),
  );
  selection.value = [];
  selConn.value = null;
  commit();
}

function duplicateSelection(): void {
  const idMap = new Map<string, string>();
  const clones: DiagramElement[] = [];
  for (const id of selection.value) {
    const el = doc.value.elements.find((e) => e.id === id);
    if (!el) continue;
    const nid = newId("el");
    idMap.set(id, nid);
    clones.push({ ...JSON.parse(JSON.stringify(el)) as DiagramElement, id: nid, x: el.x + 24, y: el.y + 24 });
  }
  const connClones: DiagramConnection[] = [];
  for (const c of doc.value.connections) {
    const ns = idMap.get(c.source.node);
    const nt = idMap.get(c.target.node);
    if (ns && nt) {
      connClones.push({
        ...JSON.parse(JSON.stringify(c)) as DiagramConnection,
        id: newId("conn"),
        source: { node: ns, port: c.source.port },
        target: { node: nt, port: c.target.port },
      });
    }
  }
  doc.value.elements.push(...clones);
  doc.value.connections.push(...connClones);
  selection.value = clones.map((c) => c.id);
  commit();
}

function copySelection(): void {
  const els = selection.value
    .map((id) => doc.value.elements.find((e) => e.id === id))
    .filter((e): e is DiagramElement => !!e)
    .map((e) => JSON.parse(JSON.stringify(e)) as DiagramElement);
  const ids = new Set(els.map((e) => e.id));
  const conns = doc.value.connections
    .filter((c) => ids.has(c.source.node) && ids.has(c.target.node))
    .map((c) => JSON.parse(JSON.stringify(c)) as DiagramConnection);
  clipboard.value = { elements: els, connections: conns };
}

function pasteClipboard(): void {
  if (!clipboard.value) return;
  const idMap = new Map<string, string>();
  const clones = clipboard.value.elements.map((el) => {
    const nid = newId("el");
    idMap.set(el.id, nid);
    return { ...JSON.parse(JSON.stringify(el)) as DiagramElement, id: nid, x: el.x + 24, y: el.y + 24 };
  });
  const connClones = clipboard.value.connections.map((c) => ({
    ...JSON.parse(JSON.stringify(c)) as DiagramConnection,
    id: newId("conn"),
    source: { node: idMap.get(c.source.node) ?? c.source.node, port: c.source.port },
    target: { node: idMap.get(c.target.node) ?? c.target.node, port: c.target.port },
  }));
  doc.value.elements.push(...clones);
  doc.value.connections.push(...connClones);
  selection.value = clones.map((c) => c.id);
  commit();
}

function selectAll(): void {
  selection.value = doc.value.elements.map((e) => e.id);
}

function applyLayout(kind: "vertical" | "horizontal" | "hierarchy"): void {
  if (kind === "vertical") layoutVertical(doc.value.elements, doc.value.connections);
  else if (kind === "horizontal") layoutHorizontal(doc.value.elements, doc.value.connections);
  else layoutHierarchy(doc.value.elements, doc.value.connections);
  commit();
}

function newDiagram(): void {
  doc.value = blankDoc();
  filePath.value = null;
  selection.value = [];
  selConn.value = null;
  history.reset(doc.value);
  syncHistFlags();
  dirty.value = false;
}

async function saveDiagram(): Promise<void> {
  error.value = null;
  try {
    let path = filePath.value;
    if (!path) {
      path = await save({ defaultPath: `${doc.value.metadata.name}.fdiag.json`, filters: [{ name: "Diagram", extensions: ["fdiag.json", "json"] }] });
      if (!path) return;
      filePath.value = path;
    }
    await invoke<string>("save_diagram", { args: { path, json: JSON.stringify(doc.value, null, 2) } });
    dirty.value = false;
  } catch (err) {
    error.value = isCommandError(err) ? `${err.kind}: ${err.message}` : String(err);
  }
}

async function saveAs(): Promise<void> {
  error.value = null;
  try {
    const path = await save({ defaultPath: `${doc.value.metadata.name}.fdiag.json`, filters: [{ name: "Diagram", extensions: ["fdiag.json", "json"] }] });
    if (!path) return;
    filePath.value = path;
    await invoke<string>("save_diagram", { args: { path, json: JSON.stringify(doc.value, null, 2) } });
    dirty.value = false;
  } catch (err) {
    error.value = isCommandError(err) ? `${err.kind}: ${err.message}` : String(err);
  }
}

async function loadDiagram(): Promise<void> {
  error.value = null;
  try {
    const picked = await open({ multiple: false, filters: [{ name: "Diagram", extensions: ["fdiag.json", "json"] }] });
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) return;
    const done = await invoke<{ path: string; json: string }>("load_diagram", { path });
    const raw: unknown = JSON.parse(done.json);
    const problem = validateDoc(raw);
    if (problem) {
      error.value = `Invalid diagram file: ${problem}`;
      return;
    }
    doc.value = raw as DiagramDoc;
    filePath.value = done.path;
    selection.value = [];
    selConn.value = null;
    history.reset(doc.value);
    syncHistFlags();
    dirty.value = false;
  } catch (err) {
    error.value = isCommandError(err) ? `${err.kind}: ${err.message}` : String(err);
  }
}

async function copySvg(): Promise<void> {
  error.value = null;
  try {
    await navigator.clipboard.writeText(svgText.value);
  } catch {
    // Fallback: select-all in the viewer textarea is manual; surface hint.
    error.value = "Clipboard blocked — select the SVG text manually and press Ctrl+C.";
  }
}

async function downloadSvg(): Promise<void> {
  error.value = null;
  try {
    const path = await save({ defaultPath: `${doc.value.metadata.name}.svg`, filters: [{ name: "SVG", extensions: ["svg"] }] });
    if (!path) return;
    await invoke<string>("export_svg_file", { args: { path, svg: svgText.value } });
  } catch (err) {
    error.value = isCommandError(err) ? `${err.kind}: ${err.message}` : String(err);
  }
}

function applyTemplate(key: string): void {
  const t = TEMPLATES.find((x) => x.key === key);
  if (!t) return;
  doc.value = t.build();
  selection.value = [];
  selConn.value = null;
  history.reset(doc.value);
  syncHistFlags();
  commit();
}

function onGlobalKey(e: KeyboardEvent): void {
  const tag = (e.target as HTMLElement)?.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
  const mod = e.ctrlKey || e.metaKey;
  if (mod && e.key.toLowerCase() === "z" && !e.shiftKey) {
    e.preventDefault();
    doUndo();
  } else if (mod && (e.key.toLowerCase() === "y" || (e.key.toLowerCase() === "z" && e.shiftKey))) {
    e.preventDefault();
    doRedo();
  } else if (mod && e.key.toLowerCase() === "c") {
    e.preventDefault();
    copySelection();
  } else if (mod && e.key.toLowerCase() === "v") {
    e.preventDefault();
    pasteClipboard();
  } else if (mod && e.key.toLowerCase() === "d") {
    e.preventDefault();
    duplicateSelection();
  } else if (mod && e.key.toLowerCase() === "a") {
    e.preventDefault();
    selectAll();
  }
}

function startLabelEdit(): void {
  if (selectedEl.value) editingLabel.value = selectedEl.value.id;
}

function commitLabelEdit(value: string): void {
  if (editingLabel.value) {
    const el = doc.value.elements.find((e) => e.id === editingLabel.value);
    if (el && el.label !== value) {
      el.label = value;
      commit();
    }
  }
  editingLabel.value = null;
}

onMounted(() => {
  window.addEventListener("keydown", onGlobalKey);
  syncHistFlags();
});
</script>

<template>
  <section class="diagram" aria-label="SVG Diagram editor">
    <div class="toolbar" role="toolbar" aria-label="Diagram tools">
      <button class="btn ghost sm" :disabled="!canUndo" title="Undo (Ctrl+Z)" @click="doUndo">↩ Undo</button>
      <button class="btn ghost sm" :disabled="!canRedo" title="Redo (Ctrl+Y)" @click="doRedo">↪ Redo</button>
      <span class="sep" />
      <button class="btn ghost sm" title="Zoom out" @click="zoom = Math.max(0.25, Math.round((zoom - 0.1) * 10) / 10)">−</button>
      <span class="zoom" aria-live="polite">{{ Math.round(zoom * 100) }}%</span>
      <button class="btn ghost sm" title="Zoom in" @click="zoom = Math.min(3, Math.round((zoom + 0.1) * 10) / 10)">+</button>
      <button class="btn ghost sm" title="Reset zoom to 100%" @click="zoom = 1">1:1</button>
      <span class="sep" />
      <button class="btn ghost sm" :class="{ on: grid }" title="Toggle grid" @click="grid = !grid">Grid</button>
      <button class="btn ghost sm" :class="{ on: snap }" title="Toggle snap-to-grid" @click="snap = !snap">Snap</button>
      <span class="sep" />
      <select class="tpl" title="Insert template" @change="applyTemplate(($event.target as HTMLSelectElement).value); ($event.target as HTMLSelectElement).value = ''">
        <option value="">Template…</option>
        <option v-for="t in TEMPLATES" :key="t.key" :value="t.key">{{ t.name }}</option>
      </select>
      <select class="tpl" title="Auto layout (explicit, undoable)" @change="applyLayout(($event.target as HTMLSelectElement).value as 'vertical' | 'horizontal' | 'hierarchy'); ($event.target as HTMLSelectElement).value = ''">
        <option value="">Layout…</option>
        <option value="vertical">Vertical flow</option>
        <option value="horizontal">Horizontal flow</option>
        <option value="hierarchy">Hierarchy</option>
      </select>
      <span class="spacer" />
      <button class="btn ghost sm" title="New diagram" @click="newDiagram">New</button>
      <button class="btn ghost sm" title="Open diagram file" @click="loadDiagram">Open</button>
      <button class="btn ghost sm" :title="dirty ? 'Save (unsaved changes)' : 'Save'" @click="saveDiagram">Save{{ dirty ? " ●" : "" }}</button>
      <button class="btn ghost sm" title="Save as…" @click="saveAs">Save As</button>
    </div>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <div class="workbench">
      <aside class="library" aria-label="Element library">
        <h4>Shapes</h4>
        <div
          v-for="d in ELEMENTS.filter((e) => ['rect', 'rounded', 'circle', 'ellipse', 'diamond', 'text'].includes(e.kind))"
          :key="d.kind"
          class="pal"
          draggable="true"
          :title="`Drag ${d.label} onto canvas (or click to add)`"
          @dragstart="onPalDrag($event, d.kind)"
          @click="addAtCenter(d.kind)"
        >
          <span class="swatch" :style="{ background: d.fill, borderColor: d.stroke }" />
          {{ d.label }}
        </div>
        <h4>Developer</h4>
        <div
          v-for="d in ELEMENTS.filter((e) => !['rect', 'rounded', 'circle', 'ellipse', 'diamond', 'text'].includes(e.kind))"
          :key="d.kind"
          class="pal"
          draggable="true"
          :title="`Drag ${d.label} onto canvas (or click to add)`"
          @dragstart="onPalDrag($event, d.kind)"
          @click="addAtCenter(d.kind)"
        >
          <span class="swatch" :style="{ background: d.fill, borderColor: d.stroke }" />
          {{ d.label }}
        </div>
        <h4>Selection</h4>
        <div class="row">
          <button class="btn ghost sm" title="Duplicate selection (Ctrl+D)" @click="duplicateSelection">Duplicate</button>
          <button class="btn ghost sm" title="Delete selection (Del)" @click="deleteSelection">Delete</button>
        </div>
        <div v-if="editingLabel" class="row">
          <input
            :value="selectedEl?.label ?? ''"
            aria-label="Edit label"
            @change="commitLabelEdit(($event.target as HTMLInputElement).value)"
            @keydown.enter="commitLabelEdit(($event.target as HTMLInputElement).value)"
            @keydown.escape="editingLabel = null"
          />
        </div>
        <button v-else class="btn ghost sm" :disabled="!selectedEl" title="Edit label (double-click canvas label also works)" @click="startLabelEdit">Edit label</button>
      </aside>
      <DiagramCanvas
        :elements="doc.elements"
        :connections="doc.connections"
        :selection="selection"
        :grid="grid"
        :snap="snap"
        :zoom="zoom"
        @select="onSelect"
        @clear-select="selection = []; selConn = null"
        @move="onMove"
        @resize="onResize"
        @drop-new="addElement"
        @connect="onConnect"
        @viewport="(_x, _y, z) => (zoom = z)"
        @delete-key="deleteSelection"
        @dblclick="startLabelEdit"
      />
      <PropsPanel :element="selectedEl" :connection="selectedConn" @patch-el="onPatchEl" @patch-conn="onPatchConn" />
    </div>
    <CodeViewer :svg="svgText" @copy="copySvg" @download="downloadSvg" />
  </section>
</template>

<style scoped>
.diagram { display: flex; flex-direction: column; gap: 0.7rem; }
.toolbar {
  display: flex; gap: 0.4rem; align-items: center; flex-wrap: wrap;
  background: var(--panel); border: 1px solid var(--line);
  border-radius: 12px; padding: 0.5rem 0.6rem;
}
.btn.sm { padding: 0.35rem 0.65rem; font-size: 0.82rem; }
.btn.ghost { background: transparent; color: var(--ink); border: 1px solid var(--line); }
.btn.ghost.on { background: var(--ink); color: white; }
.btn:disabled { opacity: 0.4; cursor: default; }
.sep { width: 1px; height: 1.4rem; background: var(--line); }
.zoom { min-width: 3rem; text-align: center; font-variant-numeric: tabular-nums; }
.tpl { border: 1px solid var(--line); border-radius: 8px; padding: 0.3rem 0.4rem; font: inherit; }
.spacer { flex: 1; }
.workbench { display: flex; gap: 0.7rem; align-items: stretch; }
.library {
  width: 150px; flex: none; background: var(--panel);
  border: 1px solid var(--line); border-radius: 12px; padding: 0.7rem;
  max-height: 560px; overflow-y: auto;
}
.library h4 { margin: 0.5rem 0 0.35rem; font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.04em; color: var(--dim); }
.library h4:first-child { margin-top: 0; }
.pal {
  display: flex; align-items: center; gap: 0.45rem;
  border: 1px solid var(--line); border-radius: 8px; padding: 0.32rem 0.45rem;
  margin-bottom: 0.3rem; cursor: grab; font-size: 0.83rem; background: #fffdf9;
}
.pal:active { cursor: grabbing; }
.swatch { width: 14px; height: 14px; border-radius: 4px; border: 2px solid; flex: none; }
.row { display: flex; gap: 0.35rem; margin-bottom: 0.4rem; }
.row input { flex: 1; border: 1px solid var(--line); border-radius: 8px; padding: 0.3rem 0.45rem; font: inherit; }
.error { color: var(--danger); }
</style>
