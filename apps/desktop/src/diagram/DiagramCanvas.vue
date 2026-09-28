<!-- SVG workspace: pan/zoom/grid/select/move/resize/connect.
Emits intent events; DiagramTab owns doc + history. All pointer math in
world units via geometry.screenToWorld. -->
<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import type { DiagramConnection, DiagramElement, ElementKind, Port } from "./types";
import { baseOf, cylinderPaths, diamondPoints } from "./elements";
import { connectionPath, dashArray, hitElement, portAt, portPoint, screenToWorld } from "./geometry";

const props = defineProps<{
  elements: DiagramElement[];
  connections: DiagramConnection[];
  selection: string[];
  grid: boolean;
  snap: boolean;
  zoom: number;
}>();

const emit = defineEmits<{
  select: [ids: string[], additive: boolean];
  clearSelect: [];
  move: [moves: Array<{ id: string; x: number; y: number }>, commit: boolean];
  resize: [id: string, x: number, y: number, w: number, h: number, commit: boolean];
  dropNew: [kind: ElementKind, x: number, y: number];
  connect: [source: { node: string; port: Port }, target: { node: string; port: Port }];
  viewport: [x: number, y: number, zoom: number];
  deleteKey: [];
}>();

const GRID = 20;
const MIN_SIZE = 30;
const svgRef = ref<SVGSVGElement | null>(null);
const pan = ref({ x: 0, y: 0 });
const spaceDown = ref(false);
/** Canvas tool: select (rubber-band) or pan (drag moves viewport). */
const tool = ref<"select" | "pan">("select");
const dragSel = ref<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
const moving = ref<{ id: string; dx: number; dy: number; origX: number; origY: number }[] | null>(null);
const resizing = ref<{ id: string; corner: string; startX: number; startY: number; orig: DiagramElement } | null>(null);
const pendingConn = ref<{ node: string; port: Port } | null>(null);
const hoverPort = ref<{ node: string; port: Port } | null>(null);
/** Live preview line end (world) while dragging a connection. */
const dragLine = ref<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
const viewBox = computed(() => {
  const w = svgRef.value?.clientWidth ?? 800;
  const h = svgRef.value?.clientHeight ?? 500;
  return `${-pan.value.x / props.zoom} ${-pan.value.y / props.zoom} ${w / props.zoom} ${h / props.zoom}`;
});

const gridLines = computed(() => {
  if (!props.grid || !svgRef.value) return { v: [] as number[], h: [] as number[] };
  const w = svgRef.value.clientWidth / props.zoom;
  const h = svgRef.value.clientHeight / props.zoom;
  const x0 = -pan.value.x / props.zoom;
  const y0 = -pan.value.y / props.zoom;
  const v: number[] = [];
  const hh: number[] = [];
  for (let x = Math.floor(x0 / GRID) * GRID; x < x0 + w; x += GRID) v.push(x);
  for (let y = Math.floor(y0 / GRID) * GRID; y < y0 + h; y += GRID) hh.push(y);
  return { v, h: hh };
});

function toWorld(e: PointerEvent): { x: number; y: number } {
  const svg = svgRef.value;
  if (!svg) return { x: 0, y: 0 };
  const rect = svg.getBoundingClientRect();
  return screenToWorld(e.clientX, e.clientY, rect, { panX: pan.value.x, panY: pan.value.y, zoom: props.zoom });
}

function snap(v: number): number {
  return props.snap ? Math.round(v / GRID) * GRID : Math.round(v);
}

function onBackgroundDown(e: PointerEvent): void {
  if (spaceDown.value || e.button === 1 || tool.value === "pan") {
    startPan(e);
    return;
  }
  const p = toWorld(e);
  dragSel.value = { x0: p.x, y0: p.y, x1: p.x, y1: p.y };
  (e.target as SVGElement).setPointerCapture?.(e.pointerId);
}

function onBackgroundMove(e: PointerEvent): void {
  if (panning.value) {
    pan.value = { x: panStart.x + (e.clientX - panStart.clientX), y: panStart.y + (e.clientY - panStart.clientY) };
    emit("viewport", 0, 0, props.zoom);
    return;
  }
  if (dragSel.value) {
    const p = toWorld(e);
    dragSel.value = { ...dragSel.value, x1: p.x, y1: p.y };
  }
}

function onBackgroundUp(e: PointerEvent): void {
  if (panning.value) {
    panning.value = false;
    return;
  }
  if (dragSel.value) {
    const { x0, y0, x1, y1 } = dragSel.value;
    if (Math.abs(x1 - x0) < 4 && Math.abs(y1 - y0) < 4) {
      emit("clearSelect");
    } else {
      const ids = props.elements
        .filter(
          (el) =>
            el.x + el.w >= Math.min(x0, x1) &&
            el.x <= Math.max(x0, x1) &&
            el.y + el.h >= Math.min(y0, y1) &&
            el.y <= Math.max(y0, y1),
        )
        .map((el) => el.id);
      emit("select", ids, e.shiftKey);
    }
    dragSel.value = null;
  }
}

const panning = ref(false);
const panStart = { x: 0, y: 0, clientX: 0, clientY: 0 };

function startPan(e: PointerEvent): void {
  panning.value = true;
  panStart.x = pan.value.x;
  panStart.y = pan.value.y;
  panStart.clientX = e.clientX;
  panStart.clientY = e.clientY;
}

function onWheel(e: WheelEvent): void {
  e.preventDefault();
  const svg = svgRef.value;
  if (!svg) return;
  const rect = svg.getBoundingClientRect();
  const mx = e.clientX - rect.left;
  const my = e.clientY - rect.top;
  const old = props.zoom;
  const next = Math.min(3, Math.max(0.25, old * (e.deltaY < 0 ? 1.1 : 1 / 1.1)));
  const wx = (mx - pan.value.x) / old;
  const wy = (my - pan.value.y) / old;
  pan.value = { x: mx - wx * next, y: my - wy * next };
  emit("viewport", 0, 0, next);
}

function onElementDown(e: PointerEvent, el: DiagramElement): void {
  e.stopPropagation();
  if (!props.selection.includes(el.id)) {
    emit("select", [el.id], e.shiftKey);
  }
  const p = toWorld(e);
  const ids = props.selection.includes(el.id) ? props.selection : [el.id];
  moving.value = ids
    .map((id) => props.elements.find((x) => x.id === id))
    .filter((x): x is DiagramElement => !!x)
    .map((m) => ({ id: m.id, dx: p.x - m.x, dy: p.y - m.y, origX: m.x, origY: m.y }));
  (e.target as SVGElement).setPointerCapture?.(e.pointerId);
}

function onPointerMove(e: PointerEvent): void {
  onBackgroundMove(e);
  if (pendingConn.value) {
    // Live preview: source port → cursor (world). Updates every move so
    // the user sees the wire while dragging (the "nothing happens" fix).
    const src = props.elements.find((x) => x.id === pendingConn.value?.node);
    const p = toWorld(e);
    if (src && pendingConn.value) {
      const q = portPoint(src, pendingConn.value.port);
      dragLine.value = { x0: q.x, y0: q.y, x1: p.x, y1: p.y };
    }
  }
  if (moving.value) {
    const mp = toWorld(e);
    const moves = moving.value.map((m) => {
      const el = props.elements.find((x) => x.id === m.id);
      if (!el) return { id: m.id, x: m.origX, y: m.origY };
      return { id: m.id, x: snap(mp.x - m.dx), y: snap(mp.y - m.dy) };
    });
    emit("move", moves, false);
  }
  if (resizing.value) {
    const p = toWorld(e);
    const r = resizing.value;
    const dx = p.x - r.startX;
    const dy = p.y - r.startY;
    let { x, y, w, h } = { x: r.orig.x, y: r.orig.y, w: r.orig.w, h: r.orig.h };
    if (r.corner.includes("e")) w = Math.max(MIN_SIZE, snap(r.orig.w + dx));
    if (r.corner.includes("s")) h = Math.max(MIN_SIZE, snap(r.orig.h + dy));
    if (r.corner.includes("w")) {
      const nw = Math.max(MIN_SIZE, snap(r.orig.w - dx));
      x = snap(r.orig.x + (r.orig.w - nw));
      w = nw;
    }
    if (r.corner.includes("n")) {
      const nh = Math.max(MIN_SIZE, snap(r.orig.h - dy));
      y = snap(r.orig.y + (r.orig.h - nh));
      h = nh;
    }
    emit("resize", r.id, Math.round(x), Math.round(y), Math.round(w), Math.round(h), false);
  }
}

function onPointerUp(e: PointerEvent): void {
  if (pendingConn.value) {
    // Release-point hit-test: pointer capture retargets pointerup to the
    // SOURCE circle, so per-circle @pointerup never fires on the target.
    // Resolve the drop in world coords instead (deterministic).
    const p = toWorld(e);
    const tol = 24 / props.zoom;
    const hit = portAt(props.elements, p, tol, pendingConn.value.node);
    if (hit) emit("connect", pendingConn.value, hit);
    pendingConn.value = null;
    dragLine.value = null;
    return;
  }
  if (panning.value) {
    panning.value = false;
  } else if (dragSel.value) {
    onBackgroundUp(e);
  }
  if (moving.value) {
    const moves = moving.value.map((m) => {
      const el = props.elements.find((x) => x.id === m.id);
      return { id: m.id, x: el?.x ?? m.origX, y: el?.y ?? m.origY };
    });
    emit("move", moves, true);
    moving.value = null;
  }
  if (resizing.value) {
    const el = props.elements.find((x) => x.id === resizing.value?.id);
    if (el && resizing.value) emit("resize", el.id, el.x, el.y, el.w, el.h, true);
    resizing.value = null;
  }
}

function onResizeDown(e: PointerEvent, el: DiagramElement, corner: string): void {
  e.stopPropagation();
  const p = toWorld(e);
  resizing.value = { id: el.id, corner, startX: p.x, startY: p.y, orig: { ...el } };
  (e.target as SVGElement).setPointerCapture?.(e.pointerId);
}

function onPortDown(e: PointerEvent, node: string, port: Port): void {
  e.stopPropagation();
  pendingConn.value = { node, port };
  dragLine.value = null;
  // NO pointer capture: capturing retargets pointerup to the source circle,
  // so the target circle's @pointerup never fires (the reported bug).
}

function onPortUp(e: PointerEvent, node: string, port: Port): void {
  e.stopPropagation();
  if (pendingConn.value && pendingConn.value.node !== node) {
    emit("connect", pendingConn.value, { node, port });
  }
  pendingConn.value = null;
  dragLine.value = null;
}
function onDragOver(e: DragEvent): void {
  // Must preventDefault AND set dropEffect: some WebViews (WebView2)
  // ignore Vue's .prevent modifier alone and never fire drop.
  e.preventDefault();
  if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
}

const KNOWN_KINDS = new Set([
  "rect", "rounded", "circle", "ellipse", "diamond", "text",
  "input", "output", "process", "database", "server", "api",
  "client", "cloud", "queue", "user", "document", "storage",
]);

function onDrop(e: DragEvent): void {
  e.preventDefault();
  const raw =
    e.dataTransfer?.getData("text/diagram-kind") || e.dataTransfer?.getData("text/plain") || "";
  const kind = raw as ElementKind | "";
  if (!kind || !KNOWN_KINDS.has(kind) || !svgRef.value) return;
  const rect = svgRef.value.getBoundingClientRect();
  const p = screenToWorld(e.clientX, e.clientY, rect, { panX: pan.value.x, panY: pan.value.y, zoom: props.zoom });
  emit("dropNew", kind, snap(p.x), snap(p.y));
}

function onKey(e: KeyboardEvent): void {
  if (e.key === " ") spaceDown.value = true;
  if ((e.key === "Delete" || e.key === "Backspace") && (e.target as HTMLElement)?.tagName !== "INPUT") {
    emit("deleteKey");
  }
  if (e.key === "Escape") {
    emit("clearSelect");
    pendingConn.value = null;
  }
}

onMounted(() => window.addEventListener("keydown", onKey));
onUnmounted(() => window.removeEventListener("keydown", onKey));

function elAt(p: { x: number; y: number }): DiagramElement | undefined {
  return [...props.elements].reverse().find((el) => hitElement(el, p));
}

function onCanvasClick(e: PointerEvent): void {
  const p = toWorld(e);
  const el = elAt(p);
  if (el && !props.selection.includes(el.id)) emit("select", [el.id], e.shiftKey);
}

const selRect = computed(() => {
  if (!dragSel.value) return null;
  const { x0, y0, x1, y1 } = dragSel.value;
  return { x: Math.min(x0, x1), y: Math.min(y0, y1), w: Math.abs(x1 - x0), h: Math.abs(y1 - y0) };
});

const PORT_LIST: Port[] = ["n", "e", "s", "w"];

function isSelected(id: string): boolean {
  return props.selection.includes(id);
}

/** Active tool for parent toolbar (v key toggles). */
function setTool(t: "select" | "pan"): void {
  tool.value = t;
}

defineExpose({ setTool, tool });
</script>

<template>
  <div class="canvas-host">
    <svg
      ref="svgRef"
      class="canvas"
      :class="{ panning: tool === 'pan' || spaceDown }"
      :viewBox="viewBox"
      @pointerdown="onBackgroundDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @click="onCanvasClick"
      @wheel="onWheel"
      @dragover="onDragOver"
      @drop="onDrop"
      role="application"
      aria-label="Diagram canvas"
    >
      <g v-if="gridLines.v.length" class="grid" aria-hidden="true">
        <line v-for="x in gridLines.v" :key="'v' + x" :x1="x" :y1="-10000" :x2="x" :y2="10000" />
        <line v-for="y in gridLines.h" :key="'h' + y" :x1="-10000" :y1="y" :x2="10000" :y2="y" />
      </g>
      <g class="conns">
        <path
          v-for="c in connections"
          :key="c.id"
          :d="connectionPath(c, elements)"
          fill="none"
          :stroke="c.color"
          :stroke-width="c.width / zoom"
          :stroke-dasharray="dashArray(c)"
          :opacity="c.opacity"
          :marker-end="c.arrow === 'end' || c.arrow === 'both' ? 'url(#darr)' : undefined"
          :marker-start="c.arrow === 'start' || c.arrow === 'both' ? 'url(#darr)' : undefined"
        />
        <line
          v-if="dragLine"
          :x1="dragLine.x0" :y1="dragLine.y0" :x2="dragLine.x1" :y2="dragLine.y1"
          stroke="#e86a2c" :stroke-width="2 / zoom" stroke-dasharray="6 4"
          pointer-events="none"
        />
      </g>
      <defs>
        <marker id="darr" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
          <path d="M 0 1 L 9 5 L 0 9 z" fill="context-stroke" />
        </marker>
      </defs>
      <g
        v-for="el in elements"
        :key="el.id"
        class="node"
        :class="{ selected: isSelected(el.id) }"
        @pointerdown="onElementDown($event, el)"
      >
        <rect
          v-if="baseOf(el.type) === 'rect'"
          :x="el.x" :y="el.y" :width="el.w" :height="el.h"
          :fill="el.fill" :stroke="isSelected(el.id) ? '#e86a2c' : el.stroke"
          :stroke-width="(isSelected(el.id) ? el.strokeWidth + 1 : el.strokeWidth) / zoom"
          :opacity="el.opacity"
        />
        <rect
          v-else-if="baseOf(el.type) === 'rounded'"
          :x="el.x" :y="el.y" :width="el.w" :height="el.h" :rx="el.radius ?? 10"
          :fill="el.fill" :stroke="isSelected(el.id) ? '#e86a2c' : el.stroke"
          :stroke-width="(isSelected(el.id) ? el.strokeWidth + 1 : el.strokeWidth) / zoom"
          :opacity="el.opacity"
        />
        <circle
          v-else-if="baseOf(el.type) === 'circle'"
          :cx="el.x + el.w / 2" :cy="el.y + el.h / 2" :r="Math.min(el.w, el.h) / 2"
          :fill="el.fill" :stroke="isSelected(el.id) ? '#e86a2c' : el.stroke"
          :stroke-width="(isSelected(el.id) ? el.strokeWidth + 1 : el.strokeWidth) / zoom"
          :opacity="el.opacity"
        />
        <ellipse
          v-else-if="baseOf(el.type) === 'ellipse'"
          :cx="el.x + el.w / 2" :cy="el.y + el.h / 2" :rx="el.w / 2" :ry="el.h / 2"
          :fill="el.fill" :stroke="isSelected(el.id) ? '#e86a2c' : el.stroke"
          :stroke-width="(isSelected(el.id) ? el.strokeWidth + 1 : el.strokeWidth) / zoom"
          :opacity="el.opacity"
        />
        <polygon
          v-else-if="baseOf(el.type) === 'diamond'"
          :points="diamondPoints(el.x, el.y, el.w, el.h)"
          :fill="el.fill" :stroke="isSelected(el.id) ? '#e86a2c' : el.stroke"
          :stroke-width="(isSelected(el.id) ? el.strokeWidth + 1 : el.strokeWidth) / zoom"
          :opacity="el.opacity"
        />
        <g v-else-if="baseOf(el.type) === 'cylinder'">
          <path
            :d="cylinderPaths(el.x, el.y, el.w, el.h).body"
            :fill="el.fill" :stroke="isSelected(el.id) ? '#e86a2c' : el.stroke"
            :stroke-width="(isSelected(el.id) ? el.strokeWidth + 1 : el.strokeWidth) / zoom"
            :opacity="el.opacity"
          />
          <path
            :d="cylinderPaths(el.x, el.y, el.w, el.h).top"
            :fill="el.fill" :stroke="isSelected(el.id) ? '#e86a2c' : el.stroke"
            :stroke-width="(isSelected(el.id) ? el.strokeWidth + 1 : el.strokeWidth) / zoom"
            :opacity="el.opacity"
          />
        </g>
        <text
          :x="el.align === 'left' ? el.x + 8 : el.align === 'right' ? el.x + el.w - 8 : el.x + el.w / 2"
          :y="el.y + el.h / 2"
          text-anchor="middle"
          dominant-baseline="central"
          :font-size="el.fontSize"
          :font-weight="el.fontWeight"
          :fill="el.textColor"
          style="pointer-events: none; user-select: none"
        >
          <tspan v-for="(ln, i) in el.label.split('\n')" :key="i" :x="el.align === 'left' ? el.x + 8 : el.align === 'right' ? el.x + el.w - 8 : el.x + el.w / 2" :dy="i === 0 ? -((el.label.split('\n').length - 1) * el.fontSize * 0.625) : el.fontSize * 1.25" :text-anchor="el.align === 'left' ? 'start' : el.align === 'right' ? 'end' : 'middle'">{{ ln }}</tspan>
        </text>
        <g v-if="isSelected(el.id) || hoverPort?.node === el.id" class="ports">
          <circle
            v-for="p in PORT_LIST"
            :key="p"
            :cx="portPoint(el, p).x"
            :cy="portPoint(el, p).y"
            :r="6 / zoom"
            class="port"
            @pointerdown="onPortDown($event, el.id, p)"
            @pointerup="onPortUp($event, el.id, p)"
            @pointerenter="hoverPort = { node: el.id, port: p }"
            @pointerleave="hoverPort = null"
          />
        </g>
        <g v-if="isSelected(el.id)" class="handles">
          <rect
            v-for="c in ['nw', 'ne', 'sw', 'se']"
            :key="c"
            :x="(c.includes('w') ? el.x : el.x + el.w) - 5 / zoom"
            :y="(c.includes('n') ? el.y : el.y + el.h) - 5 / zoom"
            :width="10 / zoom"
            :height="10 / zoom"
            class="handle"
            @pointerdown="onResizeDown($event, el, c)"
          />
        </g>
      </g>
      <rect
        v-if="selRect"
        :x="selRect.x" :y="selRect.y" :width="selRect.w" :height="selRect.h"
        class="sel-rect"
      />
    </svg>
  </div>
</template>

<style scoped>
.canvas-host {
  flex: 1 1 auto;
  min-width: 0;
  min-height: clamp(380px, 55vh, 720px);
  position: relative;
  background: #fffdf9;
  border: 1px solid var(--line);
  border-radius: 12px;
  overflow: hidden;
}
.canvas {
  width: 100%;
  height: 100%;
  min-height: inherit;
  cursor: default;
  touch-action: none;
  display: block;
}
.canvas.panning {
  cursor: grab;
}
.canvas.panning:active {
  cursor: grabbing;
}
.grid line {
  stroke: #ece7dd;
  stroke-width: 1;
}
.node {
  cursor: move;
}
.node.selected {
  filter: drop-shadow(0 0 3px rgb(232 106 44 / 0.6));
}
.port {
  fill: #ffffff;
  stroke: #e86a2c;
  stroke-width: 2;
  cursor: crosshair;
}
.port:hover {
  fill: #e86a2c;
}
.handle {
  fill: #e86a2c;
  stroke: #ffffff;
  stroke-width: 1.5;
  cursor: nwse-resize;
}
.sel-rect {
  fill: rgb(232 106 44 / 0.12);
  stroke: #e86a2c;
  stroke-dasharray: 4 3;
}
</style>
