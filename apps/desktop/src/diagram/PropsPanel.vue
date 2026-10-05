<!-- Adaptive properties: position/size/appearance/text/connection.
Emits patch events; DiagramTab applies + commits history. -->
<script setup lang="ts">
import { computed } from "vue";
import type { ArrowStyle, DashStyle, ConnKind, Port } from "./types";
import type { DiagramConnection, DiagramElement } from "./types";

const props = defineProps<{
  element: DiagramElement | null;
  connection: DiagramConnection | null;
}>();

const emit = defineEmits<{
  patchEl: [patch: Partial<DiagramElement>];
  patchConn: [patch: Partial<DiagramConnection>];
  reconnect: [end: "source" | "target", node: string, port: Port];
}>();

const title = computed(() => {
  if (props.element) return `Element · ${props.element.type}`;
  if (props.connection) return "Connection";
  return "Nothing selected";
});

function num(v: string, fallback: number): number {
  const n = Number(v);
  return Number.isFinite(n) ? n : fallback;
}
</script>

<template>
  <aside class="props" aria-label="Properties">
    <h3>{{ title }}</h3>
    <p v-if="!element && !connection" class="dim">Select an element or connection to edit its properties.</p>
    <template v-if="element">
      <div class="group">
        <h4>Position &amp; size</h4>
        <div class="grid2">
          <label>X<input type="number" :value="element.x" @change="emit('patchEl', { x: num(($event.target as HTMLInputElement).value, element.x) })" /></label>
          <label>Y<input type="number" :value="element.y" @change="emit('patchEl', { y: num(($event.target as HTMLInputElement).value, element.y) })" /></label>
          <label>W<input type="number" min="30" :value="element.w" @change="emit('patchEl', { w: Math.max(30, num(($event.target as HTMLInputElement).value, element.w)) })" /></label>
          <label>H<input type="number" min="30" :value="element.h" @change="emit('patchEl', { h: Math.max(30, num(($event.target as HTMLInputElement).value, element.h)) })" /></label>
        </div>
      </div>
      <div class="group">
        <h4>Appearance</h4>
        <label>Fill<input type="color" :value="element.fill === 'transparent' ? '#ffffff' : element.fill" @input="emit('patchEl', { fill: ($event.target as HTMLInputElement).value })" />
          <input class="hex" type="text" :value="element.fill" @change="emit('patchEl', { fill: ($event.target as HTMLInputElement).value })" spellcheck="false" /></label>
        <label>Stroke<input type="color" :value="element.stroke === 'none' ? '#000000' : element.stroke" @input="emit('patchEl', { stroke: ($event.target as HTMLInputElement).value })" />
          <input class="hex" type="text" :value="element.stroke" @change="emit('patchEl', { stroke: ($event.target as HTMLInputElement).value })" spellcheck="false" /></label>
        <label>Stroke width · {{ element.strokeWidth }}<input type="range" min="0" max="12" :value="element.strokeWidth" @input="emit('patchEl', { strokeWidth: Number(($event.target as HTMLInputElement).value) })" /></label>
        <label>Opacity · {{ Math.round(element.opacity * 100) }}%<input type="range" min="10" max="100" :value="Math.round(element.opacity * 100)" @input="emit('patchEl', { opacity: Number(($event.target as HTMLInputElement).value) / 100 })" /></label>
        <label v-if="element.radius !== undefined">Corner radius · {{ element.radius }}<input type="range" min="0" max="40" :value="element.radius" @input="emit('patchEl', { radius: Number(($event.target as HTMLInputElement).value) })" /></label>
      </div>
      <div class="group">
        <h4>Text</h4>
        <label>Label<textarea :value="element.label" rows="2" @change="emit('patchEl', { label: ($event.target as HTMLTextAreaElement).value })" /></label>
        <label>Font size · {{ element.fontSize }}<input type="range" min="8" max="48" :value="element.fontSize" @input="emit('patchEl', { fontSize: Number(($event.target as HTMLInputElement).value) })" /></label>
        <label>Weight<select :value="element.fontWeight" @change="emit('patchEl', { fontWeight: Number(($event.target as HTMLSelectElement).value) })">
          <option :value="400">Regular</option><option :value="600">Semi-bold</option><option :value="800">Bold</option>
        </select></label>
        <label>Align<select :value="element.align" @change="emit('patchEl', { align: ($event.target as HTMLSelectElement).value as DiagramElement['align'] })">
          <option value="left">Left</option><option value="center">Center</option><option value="right">Right</option>
        </select></label>
        <label>Text color<input type="color" :value="element.textColor" @input="emit('patchEl', { textColor: ($event.target as HTMLInputElement).value })" />
          <input class="hex" type="text" :value="element.textColor" @change="emit('patchEl', { textColor: ($event.target as HTMLInputElement).value })" spellcheck="false" /></label>
      </div>
    </template>
    <template v-if="connection">
      <div class="group">
        <h4>Routing</h4>
        <label>Type<select :value="connection.kind" @change="emit('patchConn', { kind: ($event.target as HTMLSelectElement).value as ConnKind })">
          <option value="straight">Straight</option><option value="orthogonal">Orthogonal</option><option value="curved">Curved</option>
        </select></label>
        <label>Arrow<select :value="connection.arrow" @change="emit('patchConn', { arrow: ($event.target as HTMLSelectElement).value as ArrowStyle })">
          <option value="none">None</option><option value="start">Start</option><option value="end">End</option><option value="both">Both</option>
        </select></label>
        <label>Line<select :value="connection.dash" @change="emit('patchConn', { dash: ($event.target as HTMLSelectElement).value as DashStyle })">
          <option value="solid">Solid</option><option value="dashed">Dashed</option><option value="dotted">Dotted</option>
        </select></label>
      </div>
      <div class="group">
        <h4>Appearance</h4>
        <label>Color<input type="color" :value="connection.color" @input="emit('patchConn', { color: ($event.target as HTMLInputElement).value })" />
          <input class="hex" type="text" :value="connection.color" @change="emit('patchConn', { color: ($event.target as HTMLInputElement).value })" spellcheck="false" /></label>
        <label>Width · {{ connection.width }}<input type="range" min="1" max="8" :value="connection.width" @input="emit('patchConn', { width: Number(($event.target as HTMLInputElement).value) })" /></label>
        <label>Opacity · {{ Math.round(connection.opacity * 100) }}%<input type="range" min="10" max="100" :value="Math.round(connection.opacity * 100)" @input="emit('patchConn', { opacity: Number(($event.target as HTMLInputElement).value) / 100 })" /></label>
      </div>
    </template>
  </aside>
</template>

<style scoped>
.props {
  width: clamp(210px, 18vw, 280px);
  flex: none;
  background: var(--panel);
  border: 1px solid var(--line-soft);
  border-radius: 12px;
  padding: 0.8rem;
  overflow-y: auto;
  max-height: 70vh;
}
@media (max-width: 900px) {
  .props { width: 100%; max-height: 260px; }
}
.props h3 { margin: 0 0 0.5rem; font-size: 0.8rem; letter-spacing: 0.05em; text-transform: uppercase; color: var(--dim); }
.props h4 { margin: 0.7rem 0 0.35rem; font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.04em; color: var(--dim); }
.group { border-top: 1px solid var(--line-soft); padding-top: 0.3rem; }
.group:first-of-type { border-top: 0; }
.grid2 { display: grid; grid-template-columns: 1fr 1fr; gap: 0.4rem; }
label { display: flex; flex-direction: column; gap: 0.2rem; font-size: 0.8rem; margin-bottom: 0.4rem; color: var(--ink-dim); }
input[type="number"], input[type="text"], select, textarea {
  border: 1px solid var(--line); border-radius: 8px; padding: 0.3rem 0.45rem; font: inherit;
  background: var(--bg-raise); color: var(--ink);
}
input[type="color"] { width: 100%; height: 28px; border: 1px solid var(--line); border-radius: 8px; padding: 0; background: none; }
.hex { font-family: ui-monospace, monospace; font-size: 0.75rem; }
.dim { color: var(--dim); font-size: 0.85rem; }
</style>
