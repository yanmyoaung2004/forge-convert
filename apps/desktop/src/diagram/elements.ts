// Element registry: 18 kinds as SVG primitives + defaults.
// Basic shapes render directly; developer components = styled base + label.
// New type = 1 row here. No editor-core change needed.
import type { DiagramElement, ElementKind } from "./types";
import { newId } from "./types";

export interface ElementDef {
  kind: ElementKind;
  label: string;
  /** SVG primitive used for rendering/export. */
  base: "rect" | "rounded" | "circle" | "ellipse" | "diamond" | "text" | "cylinder" | "actor" | "useCase" | "class" | "package" | "note" | "table" | "key" | "column" | "fkLink" | "input" | "output" | "process" | "server" | "api" | "client" | "cloud" | "queue" | "user" | "document";
  /** Palette section. */
  group: "shapes" | "developer" | "uml" | "er";
  w: number;
  h: number;
  fill: string;
  stroke: string;
  strokeWidth: number;
  radius?: number;
  fontSize?: number;
}

const BRAND = "#e86a2c";
const INK = "#1c1917";
const LINE = "#78716c";
export const ELEMENTS: ElementDef[] = [
  { kind: "rect", group: "shapes", label: "Rectangle", base: "rect", w: 140, h: 80, fill: "#ffffff", stroke: INK, strokeWidth: 2 },
  { kind: "rounded", group: "shapes", label: "Rounded", base: "rounded", w: 140, h: 80, fill: "#ffffff", stroke: INK, strokeWidth: 2, radius: 12 },
  { kind: "circle", group: "shapes", label: "Circle", base: "circle", w: 100, h: 100, fill: "#ffffff", stroke: INK, strokeWidth: 2 },
  { kind: "ellipse", group: "shapes", label: "Ellipse", base: "ellipse", w: 150, h: 90, fill: "#ffffff", stroke: INK, strokeWidth: 2 },
  { kind: "diamond", group: "shapes", label: "Diamond", base: "diamond", w: 140, h: 100, fill: "#ffffff", stroke: INK, strokeWidth: 2 },
  { kind: "text", group: "shapes", label: "Text", base: "text", w: 160, h: 40, fill: "transparent", stroke: "none", strokeWidth: 0, fontSize: 16 },
  { kind: "input", group: "developer", label: "Input", base: "input", w: 170, h: 80, fill: "#e8f0fe", stroke: "#2563eb", strokeWidth: 2, radius: 10 },
  { kind: "output", group: "developer", label: "Output", base: "output", w: 170, h: 80, fill: "#e6f4ea", stroke: "#15803d", strokeWidth: 2, radius: 10 },
  { kind: "process", group: "developer", label: "Process", base: "process", w: 170, h: 90, fill: "#fef7e0", stroke: "#b7791f", strokeWidth: 2 },
  { kind: "database", group: "developer", label: "Database", base: "cylinder", w: 170, h: 110, fill: "#f3e8fd", stroke: "#7c3aed", strokeWidth: 2 },
  { kind: "server", group: "developer", label: "Server", base: "server", w: 170, h: 100, fill: "#e8f0fe", stroke: "#1a73e8", strokeWidth: 2 },
  { kind: "api", group: "developer", label: "API", base: "api", w: 170, h: 80, fill: "#fdeede", stroke: BRAND, strokeWidth: 2, radius: 12 },
  { kind: "client", group: "developer", label: "Client", base: "client", w: 170, h: 90, fill: "#e0f2f1", stroke: "#00796b", strokeWidth: 2, radius: 12 },
  { kind: "cloud", group: "developer", label: "Cloud", base: "cloud", w: 180, h: 100, fill: "#f1f8ff", stroke: "#4285f4", strokeWidth: 2 },
  { kind: "queue", group: "developer", label: "Queue", base: "queue", w: 170, h: 80, fill: "#fff8e1", stroke: "#f9ab00", strokeWidth: 2 },
  { kind: "user", group: "developer", label: "User", base: "user", w: 110, h: 100, fill: "#fce4ec", stroke: "#c2185b", strokeWidth: 2 },
  { kind: "document", group: "developer", label: "Document", base: "document", w: 160, h: 100, fill: "#ffffff", stroke: LINE, strokeWidth: 2 },
  { kind: "storage", group: "developer", label: "Storage", base: "cylinder", w: 170, h: 100, fill: "#ede7f6", stroke: "#4527a0", strokeWidth: 2 },
  { kind: "umlClass", group: "uml", label: "Class", base: "class", w: 170, h: 120, fill: "#ffffff", stroke: INK, strokeWidth: 2, fontSize: 13 },
  { kind: "umlActor", group: "uml", label: "Actor", base: "actor", w: 90, h: 130, fill: "#ffffff", stroke: INK, strokeWidth: 2, fontSize: 12 },
  { kind: "umlUseCase", group: "uml", label: "Use Case", base: "useCase", w: 170, h: 80, fill: "#ffffff", stroke: INK, strokeWidth: 2 },
  { kind: "umlPackage", group: "uml", label: "Package", base: "package", w: 170, h: 110, fill: "#fffdf9", stroke: INK, strokeWidth: 2 },
  { kind: "umlNote", group: "uml", label: "Note", base: "note", w: 150, h: 90, fill: "#fef9c3", stroke: "#a16207", strokeWidth: 2 },
  // ERD (table + columns + key + FK edge styling via connections).
  { kind: "erTable", group: "er", label: "users", base: "table", w: 190, h: 130, fill: "#ffffff", stroke: "#1e3a8a", strokeWidth: 2, fontSize: 13 },
  { kind: "erColumn", group: "er", label: "id : INT", base: "column", w: 190, h: 30, fill: "#eff6ff", stroke: "#93c5fd", strokeWidth: 1, fontSize: 12 },
  { kind: "erKey", group: "er", label: "PK", base: "key", w: 60, h: 30, fill: "#fef3c7", stroke: "#d97706", strokeWidth: 2, fontSize: 12 },
  { kind: "erFk", group: "er", label: "FK", base: "fkLink", w: 120, h: 30, fill: "transparent", stroke: "none", strokeWidth: 0, fontSize: 12 },
];

export function defOf(kind: ElementKind): ElementDef {
  const d = ELEMENTS.find((e) => e.kind === kind);
  if (!d) throw new Error(`unknown element kind: ${kind}`);
  return d;
}

/** Create an element instance at world coords from a palette kind. */
export function createElement(kind: ElementKind, x: number, y: number): DiagramElement {
  const d = defOf(kind);
  return {
    id: newId("el"),
    type: kind,
    x: Math.round(x - d.w / 2),
    y: Math.round(y - d.h / 2),
    w: d.w,
    h: d.h,
    label: d.label,
    fill: d.fill,
    stroke: d.stroke,
    strokeWidth: d.strokeWidth,
    opacity: 1,
    radius: d.radius,
    fontSize: d.fontSize ?? 14,
    fontWeight: 400,
    textColor: INK,
    align: "center",
  };
}

/** Base primitive for a (possibly developer-styled) kind. */
export function baseOf(kind: ElementKind): ElementDef["base"] {
  return defOf(kind).base;
}

/** Diamond corner points for x/y/w/h (centered rhombus). */
export function diamondPoints(x: number, y: number, w: number, h: number): string {
  const cx = x + w / 2;
  const cy = y + h / 2;
  return `${cx},${y} ${x + w},${cy} ${cx},${y + h} ${x},${cy}`;
}

/**
 * Cylinder (disk-pack) body path: rect with elliptical top + bottom rims.
 * `rim` = ellipse ry (capped so short boxes stay sane). Returns body outline
 * + top-rim paths for shading.
 */
export function cylinderPaths(x: number, y: number, w: number, h: number): { body: string; top: string } {
  const rim = Math.max(6, Math.min(w / 2, h * 0.18));
  const r = (n: number): number => Math.round(n * 10) / 10;
  const body =
    `M ${r(x)} ${r(y + rim)} ` +
    `A ${r(w / 2)} ${r(rim)} 0 0 1 ${r(x + w)} ${r(y + rim)} ` +
    `L ${r(x + w)} ${r(y + h - rim)} ` +
    `A ${r(w / 2)} ${r(rim)} 0 0 1 ${r(x)} ${r(y + h - rim)} Z`;
  const top =
    `M ${r(x)} ${r(y + rim)} ` +
    `A ${r(w / 2)} ${r(rim)} 0 0 0 ${r(x + w)} ${r(y + rim)} ` +
    `A ${r(w / 2)} ${r(rim)} 0 0 0 ${r(x)} ${r(y + rim)} Z`;
  return { body, top };
}

/**
 * Realistic icon glyphs: small dark pictograms drawn in the element's
 * stroke color at top-left, label text shifts right/below. All coordinates
 * derive from (x, y, w, h) — resize-safe. One function per block so each
 * glyph is reviewable in isolation.
 */
function rr(x: number, y: number, w: number, h: number, rad: number): string {
  const r = Math.min(rad, w / 2, h / 2);
  return `M ${x} ${y + r} Q ${x} ${y} ${x + r} ${y} H ${x + w - r} Q ${x + w} ${y} ${x + w} ${y + r} V ${y + h - r} Q ${x + w} ${y + h} ${x + w - r} ${y + h} H ${x + r} Q ${x} ${y + h} ${x} ${y + h - r} Z`;
}

/** Server: rack box + 2 drive slots + LED dots. */
export function serverGlyph(x: number, y: number, w: number, h: number): { box: string; slots: string; leds: Array<{ cx: number; cy: number; r: number }> } {
  const iw = Math.min(44, w * 0.34);
  const ih = Math.min(40, h * 0.52);
  const ix = x + 10;
  const iy = y + (h - ih) / 2;
  const leds: Array<{ cx: number; cy: number; r: number }> = [];
  let slots = "";
  for (let i = 0; i < 2; i += 1) {
    const sy = iy + 6 + i * ((ih - 12) / 2);
    const sh = (ih - 18) / 2;
    slots += `M ${ix + 5} ${sy} H ${ix + iw - 14} V ${sy + sh} H ${ix + 5} Z `;
    leds.push({ cx: ix + iw - 8, cy: sy + sh / 2, r: 2 });
  }
  return { box: rr(ix, iy, iw, ih, 4), slots, leds };
}

/** Cloud: overlapping-circles outline (single path, fillable). */
export function cloudPath(x: number, y: number, w: number, h: number): string {
  const iw = Math.min(52, w * 0.42);
  const ih = Math.min(32, h * 0.42);
  const ix = x + 10;
  const iy = y + 8;
  const r1 = ih * 0.42;
  const cx1 = ix + iw * 0.3;
  const cy1 = iy + ih * 0.58;
  const cx2 = ix + iw * 0.55;
  const cy2 = iy + ih * 0.38;
  const r2 = ih * 0.5;
  const cx3 = ix + iw * 0.74;
  const cy3 = iy + ih * 0.6;
  const r3 = ih * 0.34;
  return (
    `M ${cx1 - r1} ${cy1} A ${r1} ${r1} 0 0 1 ${cx1} ${cy1 - r1} ` +
    `A ${r2} ${r2} 0 0 1 ${cx2 + r2 * 0.6} ${cy2 - r2 * 0.7} ` +
    `A ${r3} ${r3} 0 0 1 ${cx3 + r3} ${cy3} ` +
    `L ${cx3 + r3} ${iy + ih} H ${cx1 - r1} Z`
  );
}

/** Queue: 3 stacked message rows with dots. */
export function queueRows(x: number, y: number, w: number, h: number): { rows: string; dots: Array<{ cx: number; cy: number; r: number }> } {
  const iw = Math.min(46, w * 0.36);
  const ix = x + 10;
  const rh = Math.min(10, (h - 16) / 3 - 3);
  let rows = "";
  const dots: Array<{ cx: number; cy: number; r: number }> = [];
  for (let i = 0; i < 3; i += 1) {
    const ry = y + 10 + i * (rh + 4);
    rows += `M ${ix} ${ry} H ${ix + iw} V ${ry + rh} H ${ix} Z `;
    dots.push({ cx: ix + 6, cy: ry + rh / 2, r: 1.6 });
  }
  return { rows, dots };
}

/** User: head circle + shoulders arc. */
export function userGlyph(x: number, y: number, w: number, h: number): { head: { cx: number; cy: number; r: number }; shoulders: string } {
  const cx = x + Math.min(30, w * 0.22);
  const hr = Math.min(9, h * 0.14);
  const hy = y + 12 + hr;
  const sy = y + h - 12;
  const shoulders = `M ${cx - hr * 1.8} ${sy} A ${hr * 1.8} ${hr * 1.8} 0 0 1 ${cx + hr * 1.8} ${sy}`;
  return { head: { cx, cy: hy, r: hr }, shoulders };
}

/** Document: page with folded corner + text lines. */
export function docGlyph(x: number, y: number, w: number, h: number): { page: string; fold: string; lines: string } {
  const iw = Math.min(34, w * 0.28);
  const ih = Math.min(42, h * 0.56);
  const ix = x + 10;
  const iy = y + (h - ih) / 2;
  const f = Math.min(8, iw * 0.24);
  const page = `M ${ix} ${iy} H ${ix + iw - f} L ${ix + iw} ${iy + f} V ${iy + ih} H ${ix} Z`;
  const fold = `M ${ix + iw - f} ${iy} V ${iy + f} H ${ix + iw}`;
  let lines = "";
  for (let i = 0; i < 3; i += 1) {
    const ly = iy + ih * 0.42 + i * 6;
    lines += `M ${ix + 5} ${ly} H ${ix + iw - 5} `;
  }
  return { page, fold, lines };
}

/** Database/storage already render as cylinders; icon = small table grid. */
export function dbGrid(x: number, y: number, w: number, h: number): string {
  const iw = Math.min(40, w * 0.32);
  const ix = x + (w - iw) / 2;
  const iy = y + h * 0.42;
  const rows = 3;
  const rh = Math.min(9, (h * 0.4) / rows);
  let d = `M ${ix} ${iy} H ${ix + iw} `;
  for (let i = 1; i <= rows; i += 1) {
    d += `M ${ix} ${iy + i * rh} H ${ix + iw} `;
  }
  d += `M ${ix} ${iy} V ${iy + rows * rh} M ${ix + iw / 2} ${iy} V ${iy + rows * rh} M ${ix + iw} ${iy} V ${iy + rows * rh}`;
  return d;
}

/** API: `</>` chevrons + slash. Client: monitor + stand. Input/output: arrows. */
export function apiGlyph(x: number, y: number): string {
  const ix = x + 12;
  const my = y + 22;
  return `M ${ix + 10} ${my - 8} L ${ix} ${my} L ${ix + 10} ${my + 8} M ${ix + 22} ${my - 8} L ${ix + 32} ${my} L ${ix + 22} ${my + 8} M ${ix + 19} ${my - 10} L ${ix + 13} ${my + 10}`;
}
export function clientGlyph(x: number, y: number, w: number, h: number): { screen: string; stand: string } {
  const iw = Math.min(44, w * 0.34);
  const ih = Math.min(30, h * 0.4);
  const ix = x + 10;
  const iy = y + 8;
  return {
    screen: rr(ix, iy, iw, ih, 3),
    stand: `M ${ix + iw / 2} ${iy + ih} V ${iy + ih + 8} M ${ix + iw * 0.3} ${iy + ih + 8} H ${ix + iw * 0.7}`,
  };
}
export function ioArrow(x: number, y: number, dir: 1 | -1): string {
  const ix = x + 12;
  const my = y + 22;
  return dir === 1
    ? `M ${ix} ${my} H ${ix + 26} M ${ix + 18} ${my - 7} L ${ix + 27} ${my} L ${ix + 18} ${my + 7}`
    : `M ${ix + 27} ${my} H ${ix + 1} M ${ix + 9} ${my - 7} L ${ix} ${my} L ${ix + 9} ${my + 7}`;
}
/** Process: gear-ish cog (circle + 4 spokes). */
export function gearGlyph(x: number, y: number): { ring: { cx: number; cy: number; r: number }; spokes: string } {
  const cx = x + 26;
  const cy = y + 22;
  const r = 9;
  let spokes = "";
  for (let i = 0; i < 4; i += 1) {
    const a = (Math.PI / 4) * i;
    const c = Math.cos(a);
    const s = Math.sin(a);
    spokes += `M ${cx - c * (r + 5)} ${cy - s * (r + 5)} L ${cx - c * r} ${cy - s * r} M ${cx + c * r} ${cy + s * r} L ${cx + c * (r + 5)} ${cy + s * (r + 5)} `;
  }
  return { ring: { cx, cy, r }, spokes };
}
