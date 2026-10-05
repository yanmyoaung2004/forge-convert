# UI redesign — dark sidebar premium world (v0.8.0)

> Direction locked by user: Dark premium + sidebar rail + shell/core-first.
> Mode: Operate (task completion; scanability > expression).
> Gates every phase: `vue-tsc --noEmit` + `vite build` + visual check in running app (`tauri dev`).

## Direction contract

- THESIS: a calm dark workbench where files move left→right (queue → options → action → results).
  Refuses the current stacked-everything panel and pill-tab bar.
- OWN-WORLD: deep charcoal ground `#12100e`, warm paper text `#f3ede3`, single ember accent `#e86a2c`;
  hairline borders `rgb(255 255 255 / 0.08)`, 12px cards, 8px controls; system sans only
  (Segoe UI / system-ui); tabular numerals for sizes/dims; inline SVG icon rail (no emoji).
- STORY: user drops files, picks format in one glanceable rail, hits one Convert button,
  sees per-file savings. PDF/History share the same rails-and-stage grammar.
- FIRST VIEWPORT: left icon rail (Convert/PDF/Diagram/Tools/History) → file queue column →
  options column → sticky Convert action bar; results land in-stage, never below a fold of cards.
- FORM: Linear/Vercel-grade Operate shell; form position 1 of 1 (no tournament — user locked it).
- FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review,
  the verdict, DESIGN.md, and every shipping raster carrying its provenance.

## What stays untouched

- All Rust commands + `api.ts` shapes; all conversion logic; Tools/Diagram internals this round
  (they inherit shell + tokens only); no copy changes beyond labels the layout forces.

## Phases (one commit each)

1. **Tokens + shell**: `:root` dark tokens, sidebar rail (SVG icons, active state, keyboard tabs),
   stage layout, shared classes (`.btn/.ghost/.primary/.card/.field/.check/.chip`), focus rings,
   scrollbars, selection color. Convert tab restructured into queue/options/action columns.
2. **PDF + History**: same grammar (queue → options → action → results); history rows with status pills.
3. **Verify + fix**: run in `tauri dev` at 960×680 + wide, fix contrast/overflow/focus issues in one batch.
4. **Follow-up (not this round)**: Tools grid polish + Diagram chrome to match; DESIGN.md.

## Craft-floor notes (binding)

- Contrast ≥4.5:1 body; no emoji-as-icons (draw 5 inline SVGs); no `border-left` alerts;
  elevation = border OR shadow, never both stacked; radii 12–16px cards, pills for chips only.
- One authored motion moment (progress + convert transition), exponential ease-out.
- States: hover/disabled/loading/error/empty all present; empty states keep icon + one-line copy.

## Risks

- Dark text on file thumbnails: keep cards slightly lifted (`#1c1917`) with hairline borders.
- 960×680 tight for 3 columns: queue collapses to top row under 900px width (stack, not squeeze).
