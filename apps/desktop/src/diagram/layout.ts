// Deterministic auto-layout: vertical / horizontal / hierarchy.
// Pure functions over elements+connections. Explicit trigger only.
import type { DiagramConnection, DiagramElement } from "./types";

const GAP_X = 80;
const GAP_Y = 70;

/** Roots = nodes with no incoming connection. Falls back to first element. */
function roots(elements: DiagramElement[], conns: DiagramConnection[]): string[] {
  const targeted = new Set(conns.map((c) => c.target.node));
  const r = elements.filter((e) => !targeted.has(e.id)).map((e) => e.id);
  return r.length > 0 ? r : elements.slice(0, 1).map((e) => e.id);
}

/** Depth of each node via BFS from roots (cycles capped at first visit). */
function depths(elements: DiagramElement[], conns: DiagramConnection[]): Map<string, number> {
  const out = new Map<string, string[]>();
  for (const c of conns) {
    const l = out.get(c.source.node) ?? [];
    l.push(c.target.node);
    out.set(c.source.node, l);
  }
  const depth = new Map<string, number>();
  const queue: Array<[string, number]> = roots(elements, conns).map((id) => [id, 0]);
  for (const [id, d] of queue) {
    if (depth.has(id)) continue;
    depth.set(id, d);
    for (const next of out.get(id) ?? []) queue.push([next, d + 1]);
  }
  // Unreached nodes go last.
  let max = depth.size > 0 ? Math.max(...depth.values()) : 0;
  for (const e of elements) {
    if (!depth.has(e.id)) {
      max += 1;
      depth.set(e.id, max);
    }
  }
  return depth;
}

function applyPositions(elements: DiagramElement[], pos: Map<string, { x: number; y: number }>): void {
  for (const el of elements) {
    const p = pos.get(el.id);
    if (p) {
      el.x = Math.round(p.x);
      el.y = Math.round(p.y);
    }
  }
}

/** Vertical flow: columns by depth, centered per level. */
export function layoutVertical(elements: DiagramElement[], conns: DiagramConnection[]): void {
  const depth = depths(elements, conns);
  const levels = new Map<number, DiagramElement[]>();
  for (const el of elements) {
    const d = depth.get(el.id) ?? 0;
    const l = levels.get(d) ?? [];
    l.push(el);
    levels.set(d, l);
  }
  const pos = new Map<string, { x: number; y: number }>();
  let y = 40;
  for (const d of [...levels.keys()].sort((a, b) => a - b)) {
    const row = levels.get(d) ?? [];
    const totalW = row.reduce((s, e) => s + e.w, 0) + GAP_X * (row.length - 1);
    let x = -totalW / 2;
    let rowH = 0;
    for (const el of row) {
      pos.set(el.id, { x, y });
      x += el.w + GAP_X;
      rowH = Math.max(rowH, el.h);
    }
    y += rowH + GAP_Y;
  }
  applyPositions(elements, pos);
}

/** Horizontal flow: transpose of vertical. */
export function layoutHorizontal(elements: DiagramElement[], conns: DiagramConnection[]): void {
  const depth = depths(elements, conns);
  const levels = new Map<number, DiagramElement[]>();
  for (const el of elements) {
    const d = depth.get(el.id) ?? 0;
    const l = levels.get(d) ?? [];
    l.push(el);
    levels.set(d, l);
  }
  const pos = new Map<string, { x: number; y: number }>();
  let x = 40;
  for (const d of [...levels.keys()].sort((a, b) => a - b)) {
    const col = levels.get(d) ?? [];
    const totalH = col.reduce((s, e) => s + e.h, 0) + GAP_Y * (col.length - 1);
    let y = -totalH / 2;
    let colW = 0;
    for (const el of col) {
      pos.set(el.id, { x, y });
      y += el.h + GAP_Y;
      colW = Math.max(colW, el.w);
    }
    x += colW + GAP_X;
  }
  applyPositions(elements, pos);
}

/** Basic hierarchy: same as vertical (parents above children). Kept as a
 * named entry so the UI offers Hierarchy explicitly per the brief. */
export function layoutHierarchy(elements: DiagramElement[], conns: DiagramConnection[]): void {
  layoutVertical(elements, conns);
}
