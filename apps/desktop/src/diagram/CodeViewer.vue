<!-- SVG source viewer: auto-refreshes, copy + download. -->
<script setup lang="ts">
import { ref } from "vue";

defineProps<{ svg: string }>();
const emit = defineEmits<{ copy: []; download: [] }>();
const copied = ref(false);

function onCopy(): void {
  copied.value = true;
  emit("copy");
  window.setTimeout(() => (copied.value = false), 1500);
}
</script>

<template>
  <section class="code" aria-label="SVG source">
    <div class="code-head">
      <strong>SVG</strong>
      <span class="dim">{{ svg.length }} chars</span>
      <span class="spacer" />
      <button class="btn ghost sm" @click="onCopy" :title="copied ? 'Copied!' : 'Copy SVG source'">{{ copied ? "✓ Copied" : "Copy" }}</button>
      <button class="btn ghost sm" @click="emit('download')" title="Download .svg file">Download</button>
    </div>
    <textarea class="src" readonly :value="svg" spellcheck="false" aria-label="Generated SVG source" />
  </section>
</template>

<style scoped>
.code {
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 12px;
  padding: 0.7rem 0.8rem;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}
.code-head { display: flex; align-items: center; gap: 0.6rem; }
.spacer { flex: 1; }
.src {
  width: 100%;
  min-height: 110px;
  max-height: 180px;
  font-family: ui-monospace, "Cascadia Code", Consolas, monospace;
  font-size: 0.72rem;
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 0.5rem;
  resize: vertical;
  background: #fffdf9;
}
.btn.sm { padding: 0.35rem 0.7rem; font-size: 0.82rem; }
</style>
