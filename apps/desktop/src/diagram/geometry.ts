// Canvas geometry: single coordinate pair + port points + path builders.
// World units = SVG user units, independent of screen px. ALL pointer math
// goes through screenToWorld (zoom-around-pointer, drag at any zoom).
import type { DiagramElement, DiagramConnection, Port } from "./types";

export interface Pt {
  x: number;
  y: number;
}

/** View transform state (pan offset in screen px + zoom). */
export interface View {
  panX: number;
  panY: number;
  zoom: number;
}

/** Screen px → world units. */
export function screenToWorld(sx: number, sy: number, rect: DOMRect, view: View): Pt {
  return {
    x: (sx - rect.left - view.panX) / view.zoom,
    y: (sy - rect.top - view.panY) / view.zoom,
  };
}

export function worldToScreen(wx: number, wy: number, view: View): Pt {
  return { x: wx * view.zoom + view.panX, y: wy * view.zoom + view.panY };
}
/** Port anchor on an element's border midpoint. */
export function portPoint(el: DiagramElement, port: Port): Pt {
  switch (port) {
    case "n":
      return { x: el.x + el.w / 2, y: el.y };
    case "s":
      return { x: el.x + el.w / 2, y: el.y + el.h };
    case "w":
      return { x: el.x, y: el.y + el.h / 2 };
    case "e":
      return { x: el.x + el.w, y: el.y + el.h / 2 };
  }
}

const PORTS: Port[] = ["n", "e", "s", "w"];

/**
 * Nearest port within `tol` world units of `p` (any element but `exclude`).
 * Drag-release hit-testing: pointer-capture retargets pointerup to the
 * source circle, so Vue's per-circle @pointerup never fires on the target.
 * Hit-testing the release point in world coords fixes it deterministically.
 */
export function portAt(
  elements: DiagramElement[],
  p: Pt,
  tol: number,
  exclude?: string,
): { node: string; port: Port } | null {
  let best: { node: string; port: Port } | null = null;
  let bestD = tol;
  for (const el of elements) {
    if (el.id === exclude) continue;
    for (const port of PORTS) {
      const q = portPoint(el, port);
      const d = Math.hypot(q.x - p.x, q.y - p.y);
      if (d <= bestD) {
        bestD = d;
        best = { node: el.id, port };
      }
    }
  }
  return best;
}

/** Bounding-box hit test in world coords. */
export function hitElement(el: DiagramElement, p: Pt): boolean {
  return p.x >= el.x && p.x <= el.x + el.w && p.y >= el.y && p.y <= el.y + el.h;
}

/** Snap a value to grid when enabled. */
export function snapVal(v: number, grid: number, enabled: boolean): number {
  return enabled ? Math.round(v / grid) * grid : v;
}

function byId<T extends { id: string }>(list: T[], id: string): T | undefined {
  return list.find((e) => e.id === id);
}

/**
 * Build an SVG path `d` for a connection between resolved port points.
 * straight = line; orthogonal = exits along the SOURCE port axis, enters
 * along the TARGET port axis, single elbow (2 elbows when axes are
 * parallel-but-offset); curved = cubic with port-aware control offset.
 * Deterministic, no autorouting.
 */
export function connectionPath(
  conn: DiagramConnection,
  elements: DiagramElement[],
): string {
  const a = byId(elements, conn.source.node);
  const b = byId(elements, conn.target.node);
  if (!a || !b) return "";
  const p = portPoint(a, conn.source.port);
  const q = portPoint(b, conn.target.port);
  if (conn.kind === "straight") return `M ${r(p.x)} ${r(p.y)} L ${r(q.x)} ${r(q.y)}`;
  if (conn.kind === "curved") {
    // Control points extend along each port's outward axis so curves leave
    // top/bottom vertically and left/right horizontally (all 4 sides work).
    const dp = portDir(conn.source.port);
    const dq = portDir(conn.target.port);
    const dist = Math.max(Math.hypot(q.x - p.x, q.y - p.y) * 0.4, 30);
    const c1 = { x: p.x + dp.x * dist, y: p.y + dp.y * dist };
    const c2 = { x: q.x + dq.x * dist, y: q.y + dq.y * dist };
    return `M ${r(p.x)} ${r(p.y)} C ${r(c1.x)} ${r(c1.y)}, ${r(c2.x)} ${r(c2.y)}, ${r(q.x)} ${r(q.y)}`;
  }
  // orthogonal: exit along source axis to the target's entry line, then
  // enter along target axis. One elbow when axes cross, two when parallel.
  const dp = portDir(conn.source.port);
  const dq = portDir(conn.target.port);
  const srcHoriz = dp.x !== 0;
  const dstHoriz = dq.x !== 0;
  if (srcHoriz !== dstHoriz) {
    // Perpendicular axes: single elbow at (exit-line, entry-line) crossing.
    const mx = srcHoriz ? q.x : p.x;
    const my = srcHoriz ? p.y : q.y;
    return `M ${r(p.x)} ${r(p.y)} L ${r(mx)} ${r(my)} L ${r(q.x)} ${r(q.y)}`;
  }
  // Parallel axes: mid-bend between the points.
  const mx = (p.x + q.x) / 2;
  const my = (p.y + q.y) / 2;
  if (srcHoriz) {
    return `M ${r(p.x)} ${r(p.y)} L ${r(mx)} ${r(p.y)} L ${r(mx)} ${r(q.y)} L ${r(q.x)} ${r(q.y)}`;
  }
  return `M ${r(p.x)} ${r(p.y)} L ${r(p.x)} ${r(my)} L ${r(q.x)} ${r(my)} L ${r(q.x)} ${r(q.y)}`;
}

/** Outward unit vector of a port (for curve controls + elbow axes). */
function portDir(port: Port): Pt {
  if (port === "e") return { x: 1, y: 0 };
  if (port === "w") return { x: -1, y: 0 };
  if (port === "s") return { x: 0, y: 1 };
  return { x: 0, y: -1 };
}

/** Dash pattern for a line style (empty = solid). */
export function dashArray(conn: Pick<DiagramConnection, "dash">): string {
  if (conn.dash === "dashed") return "8 5";
  if (conn.dash === "dotted") return "2 4";
  return "";
}

function r(n: number): number {
  return Math.round(n * 10) / 10;
}

export { PORTS };
