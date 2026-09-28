<!-- SVG source: bidirectional. Editing the box + Import loads pasted SVG
as editable blocks (svgToDoc); canvas edits re-render here. -->
<script setup lang="ts">
import { ref, watch } from "vue";

const props = defineProps<{ svg: string }>();
const emit = defineEmits<{ copy: []; download: []; importSvg: [text: string] }>();
const copied = ref(false);
const draft = ref(props.svg);
const dirty = ref(false);

watch(
  () => props.svg,
  (next) => {
    // Don't clobber the user's paste while they type: only refresh when
    // the box matches the last rendered value (i.e. not being edited).
    if (!dirty.value) draft.value = next;
  },
);

function onCopy(): void {
  copied.value = true;
  emit("copy");
  window.setTimeout(() => (copied.value = false), 1500);
}

function onImport(): void {
  emit("importSvg", draft.value);
  dirty.value = false;
}
</script>

<template>
  <section class="code" aria-label="SVG source">
    <div class="code-head">
      <strong>SVG</strong>
      <span class="dim">{{ svg.length }} chars</span>
      <span v-if="dirty" class="edited" title="Edited — Import to load as blocks, or Revert">● edited</span>
      <span class="spacer" />
      <button class="btn ghost sm" @click="onImport" title="Parse the box content into editable blocks">Import → blocks</button>
      <button class="btn ghost sm" @click="draft = svg; dirty = false" title="Discard edits, show current canvas SVG">Revert</button>
      <button class="btn ghost sm" @click="onCopy" :title="copied ? 'Copied!' : 'Copy SVG source'">{{ copied ? "✓ Copied" : "Copy" }}</button>
      <button class="btn ghost sm" @click="emit('download')" title="Download .svg file">Download</button>
    </div>
    <textarea
      class="src"
      v-model="draft"
      spellcheck="false"
      aria-label="SVG source (editable — Import loads it as blocks)"
      @input="dirty = true"
    ></textarea>
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
.edited { color: var(--brand-deep); font-size: 0.8rem; font-weight: 700; }
.spacer { flex: 1; }
.src {
  width: 100%;
  min-height: 110px;
  max-height: 180px;
  font-family: ui-monospace, "Cascadia Code", Consolas, monospace;
  font-size: 0.72rem;
  border: 1px solid var(--line);
  border-radius: 8px;
  resize: vertical;
  background: #fffdf9;
}
.btn.sm { padding: 0.35rem 0.7rem; font-size: 0.82rem; }
</style>
