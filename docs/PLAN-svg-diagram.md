# SVG Diagram — Detailed Implementation Plan

> Fact-first plan. Stack facts verified 2026-09-28 from this repo (not assumed):
> Vue 3.5 + TS ~6.0.3 + Vite 8 + Tauri 2.11 (`apps/desktop/package.json`);
> NO router (tab is a `ref` in `App.vue:42`), NO store (per-tab `ref`s),
> NO undo infra, NO modal system, NO frontend test runner (gates are
> `vue-tsc --noEmit` + `vite build`), NO fs plugin (capabilities are
> `core:default, dialog:default, opener:default` only), NO C++ anywhere
> (grep finds only `winapi` transitive crates — the brief's "C++" does not
> exist in this tree). Backend pattern: thin job-level `#[tauri::command]`s
> over shared engine, JSON-safe `{kind,message}` errors (`commands.rs:37-71`).
> Theme: CSS vars `--bg/--panel/--ink/--dim/--line/--brand/--ok/--danger`
> + classes `.btn/.ghost/.primary/.card/.panel/.field/.check/.tabs`
> (`App.vue` scoped styles). Window 960×680 (`tauri.conf.json`).

## Architecture decision (why, not just what)

- **Frontend-only document + history; Rust only for file bytes.**
  Diagram editing is UI state (pointer, selection, viewport), not conversion —
  forcing it through the engine would create a fake "engine" with no domain.
  Rust gets 3 thin commands reusing `StdFileSystem::write_atomic/read`
  (`save_diagram/load_diagram/export_svg`) + existing `plugin-dialog`
  `save()/open()` on the TS side. Zero new dependencies (no fs plugin, no
  diagram framework, no layout lib). Clipboard via `navigator.clipboard`.
- **Pure-TS core, DOM-free:** `types.ts` (model) + `geometry.ts` (coords,
  routing) + `serialize.ts` (SVG + JSON validation) + `layout.ts` have NO
  Vue/DOM imports → smoke-testable with `bun` (present per PROGRESS.md),
  no vitest needed.
- **History = snapshot stack** (not command objects): diagrams are small
  (<1000 nodes expected); `JSON.stringify` snapshot per commit is simple,
  reliable, and trivially undoable for ALL ops incl. auto-layout. Cap 100.
- **Coordinates:** internal world units = SVG user units, independent of
  screen px. Single `screenToWorld`/`worldToScreen` pair in `geometry.ts`;
  all pointer math goes through it (zoom-around-pointer, drag at any zoom).
- **Routing (predictable, no engine):** straight = line port→port;
  orthogonal = H-then-V (or V-then-H by side) mid-bend; curved = cubic with
  horizontal control offset. Ports = N/E/S/W midpoints; connection stores
  `source:{node,port}, target:{node,port}` so moves/resizes re-derive path.
- **Layout (deterministic):** vertical = topological columns by depth,
  horizontal = transpose; hierarchy = BFS levels from roots, centered per
  level. Explicit trigger only, undoable via snapshot.

## Document model (v1, stable IDs)

```ts
interface DiagramDoc { version: 1; elements: El[]; connections: Conn[];
  viewport: { x: number; y: number; zoom: number }; metadata: {...} }
El = { id, type, x, y, w, h, label, fill, stroke, strokeWidth, opacity,
  radius?, fontSize?, fontWeight?, textColor?, align? }
Conn = { id, source: {node,port}, target: {node,port}, kind: straight|orthogonal|curved,
  color, width, dash: solid|dashed|dotted, arrow: none|start|end|both, opacity }
```

IDs: `crypto.randomUUID()` (browser + WebView2 present). Validation on load:
version must be 1, arrays present, numbers finite — else graceful error, never crash.

## Element library (all SVG primitives, extensible registry)

Basic: rectangle, rounded-rect, circle, ellipse, diamond, text.
Developer: input, output, process, database, server, api, client, cloud,
queue, user, document, storage — each = `{ base: rect|ellipse|..., defaults,
label }` in `elements.ts`; adding a type = 1 registry row, no core change.

## Files (all new, none touch existing logic)

```
apps/desktop/src/diagram/
  types.ts       # model + ID gen + validation
  elements.ts    # registry (18 types, defaults, diamond path, db glyph)
  geometry.ts    # screenToWorld, port points, path builders, hit/snap
  serialize.ts   # doc→SVG string (viewBox, markers, escaping), JSON save/load
  history.ts     # snapshot undo/redo stack
  layout.ts      # vertical/horizontal/hierarchy (pure)
  templates.ts   # 7 templates (flow, software-arch, api, ai-agent, pipeline, hierarchy, db)
  DiagramTab.vue # toolbar + library + canvas host + props + code drawer
  DiagramCanvas.vue # SVG workspace (pan/zoom/grid/select/move/resize/connect)
  PropsPanel.vue    # adaptive position/size/appearance/text/connection
  CodeViewer.vue    # SVG source + copy + download
apps/desktop/src-tauri/src/diagram.rs  # save/load/export commands (StdFileSystem)
```

`App.vue` change: tab union + 1 button + `<DiagramTab v-if>` (≤10 lines).
`api.ts` change: `save/load/exportSvg` wrappers + `saveDiagram()` dialog helper.
`lib.rs` change: register 3 commands. Nothing else touched.

## Phase plan (each = code + gates + ONE commit)

| # | Phase | Acceptance |
|---|-------|-----------|
| 1 | Repo understanding (this doc) | plan committed |
| 2 | Foundation: `types/elements/geometry` + Rust `diagram.rs` + registrations | `cargo check`, `vue-tsc` green |
| 3 | Canvas+library: `DiagramCanvas` (pan/zoom/grid/select/move) + library DnD + `DiagramTab` shell in nav | tab opens, drag box, move it |
| 4 | Resize/text/connect: handles, dbl-click label, ports, 3 routing kinds, arrows/dash | 2 boxes + curved arrow, move keeps it |
| 5 | Props panel + history (undo/redo/copy/paste/duplicate) | edit color/size, Ctrl+Z reverts |
| 6 | Layout + templates + persistence (save/load JSON) | template loads, auto-layout, reopen file |
| 7 | SVG output (serialize + viewer + copy/download) + shortcuts + error states | valid SVG pastes into README |
| 8 | Polish: tooltips, a11y labels, perf pass, smoke tests, docs | full gates green |

Gates every phase: `cargo fmt --check` (if Rust touched) +
`cargo clippy --workspace --all-targets -- -D warnings` (if Rust touched) +
`cargo test --workspace --exclude forgeconvert` (if Rust touched) +
`pnpm --dir apps/desktop exec vue-tsc --noEmit` + `vite build`.
Smoke: `bun` script exercising geometry/serialize/layout/history (DOM-free).

## Non-goals (v1)

Mermaid, SVG import, PNG/PDF export, collab, cloud, accounts, path editing,
advanced routing, plugin marketplace. Architecture (pure core + registry +
isolated routing) keeps them addable later.

## Risks (facts, not guesses)

- `plugin-dialog` `save()` API shape: verify against vendored
  `@tauri-apps/plugin-dialog` .d.ts before coding (not from memory).
- `crypto.randomUUID` in WebView2: verify at runtime; fallback counter kept.
- `navigator.clipboard` in Tauri webview: needs focus; fallback = select-all
  in textarea + `document.execCommand('copy')`.
- Window 960×680 is tight for 3-pane editor: canvas min-height + horizontal
  scroll; no `tauri.conf.json` change in v1.
