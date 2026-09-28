// Element registry: 18 kinds as SVG primitives + defaults.
// Basic shapes render directly; developer components = styled base + label.
// New type = 1 row here. No editor-core change needed.
import type { DiagramElement, ElementKind } from "./types";
import { newId } from "./types";

export interface ElementDef {
  kind: ElementKind;
  label: string;
  /** SVG primitive used for rendering/export. */
  base: "rect" | "rounded" | "circle" | "ellipse" | "diamond" | "text" | "cylinder" | "actor" | "useCase" | "class" | "package" | "note" | "table" | "key" | "column" | "fkLink";
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
  { kind: "input", group: "developer", label: "Input", base: "rounded", w: 150, h: 70, fill: "#e8f0fe", stroke: "#2563eb", strokeWidth: 2, radius: 10 },
  { kind: "output", group: "developer", label: "Output", base: "rounded", w: 150, h: 70, fill: "#e6f4ea", stroke: "#15803d", strokeWidth: 2, radius: 10 },
  { kind: "process", group: "developer", label: "Process", base: "rect", w: 150, h: 80, fill: "#fef7e0", stroke: "#b7791f", strokeWidth: 2 },
  { kind: "database", group: "developer", label: "Database", base: "cylinder", w: 150, h: 100, fill: "#f3e8fd", stroke: "#7c3aed", strokeWidth: 2 },
  { kind: "server", group: "developer", label: "Server", base: "rect", w: 150, h: 90, fill: "#e8f0fe", stroke: "#1a73e8", strokeWidth: 2 },
  { kind: "api", group: "developer", label: "API", base: "rounded", w: 150, h: 70, fill: "#fdeede", stroke: BRAND, strokeWidth: 2, radius: 12 },
  { kind: "client", group: "developer", label: "Client", base: "rounded", w: 140, h: 80, fill: "#e0f2f1", stroke: "#00796b", strokeWidth: 2, radius: 12 },
  { kind: "cloud", group: "developer", label: "Cloud", base: "ellipse", w: 160, h: 90, fill: "#f1f8ff", stroke: "#4285f4", strokeWidth: 2 },
  { kind: "queue", group: "developer", label: "Queue", base: "rect", w: 150, h: 70, fill: "#fff8e1", stroke: "#f9ab00", strokeWidth: 2 },
  { kind: "user", group: "developer", label: "User", base: "circle", w: 90, h: 90, fill: "#fce4ec", stroke: "#c2185b", strokeWidth: 2 },
  { kind: "document", group: "developer", label: "Document", base: "rect", w: 130, h: 90, fill: "#ffffff", stroke: LINE, strokeWidth: 2 },
  { kind: "storage", group: "developer", label: "Storage", base: "cylinder", w: 150, h: 90, fill: "#ede7f6", stroke: "#4527a0", strokeWidth: 2 },
  // UML (class + use-case essentials; simple SVG, same styling pipeline).
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
