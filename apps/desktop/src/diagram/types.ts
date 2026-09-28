// SVG Diagram document model — pure types, no Vue/DOM.
// v1: version + elements + connections + viewport + metadata. Stable IDs.

/** Document version. Loaders reject anything else with a useful message. */
export const DIAGRAM_VERSION = 1 as const;

/** Element kinds. Basic shapes + developer components (registry in elements.ts). */
export type ElementKind =
  | "rect"
  | "rounded"
  | "circle"
  | "ellipse"
  | "diamond"
  | "text"
  | "input"
  | "output"
  | "process"
  | "database"
  | "server"
  | "api"
  | "client"
  | "cloud"
  | "queue"
  | "user"
  | "document"
  | "storage";

/** Text alignment inside an element. */
export type TextAlign = "left" | "center" | "right";

/** One diagram element. All numbers are world units (SVG user units). */
export interface DiagramElement {
  id: string;
  type: ElementKind;
  x: number;
  y: number;
  w: number;
  h: number;
  label: string;
  fill: string;
  stroke: string;
  strokeWidth: number;
  opacity: number;
  /** Corner radius (rounded rects; ignored elsewhere). */
  radius?: number;
  fontSize: number;
  fontWeight: number;
  textColor: string;
  align: TextAlign;
}

/** Node side a connection attaches to. */
export type Port = "n" | "e" | "s" | "w";

/** Connection path routing. */
export type ConnKind = "straight" | "orthogonal" | "curved";

/** Arrowhead placement. */
export type ArrowStyle = "none" | "start" | "end" | "both";

/** Line dash pattern. */
export type DashStyle = "solid" | "dashed" | "dotted";

/** One persistent connection between two element ports. */
export interface DiagramConnection {
  id: string;
  source: { node: string; port: Port };
  target: { node: string; port: Port };
  kind: ConnKind;
  color: string;
  width: number;
  dash: DashStyle;
  arrow: ArrowStyle;
  opacity: number;
}

/** Canvas viewport: top-left world coords + zoom factor. */
export interface Viewport {
  x: number;
  y: number;
  zoom: number;
}

/** Editable diagram document (project file = JSON of this). */
export interface DiagramDoc {
  version: typeof DIAGRAM_VERSION;
  elements: DiagramElement[];
  connections: DiagramConnection[];
  viewport: Viewport;
  metadata: { name: string; createdAt: string; updatedAt: string };
}

/** Stable unique ID. Prefers crypto.randomUUID, falls back to a counter. */
let fallbackCounter = 0;
export function newId(prefix: string): string {
  try {
    const uuid =
      typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : null;
    if (uuid) return `${prefix}-${uuid.slice(0, 8)}`;
  } catch {
    /* fall through to counter */
  }
  fallbackCounter += 1;
  return `${prefix}-${Date.now().toString(36)}-${fallbackCounter}`;
}

/** Blank document with sane defaults. */
export function blankDoc(name = "Untitled diagram"): DiagramDoc {
  const now = new Date().toISOString();
  return {
    version: DIAGRAM_VERSION,
    elements: [],
    connections: [],
    viewport: { x: 0, y: 0, zoom: 1 },
    metadata: { name, createdAt: now, updatedAt: now },
  };
}

/** Validate unknown JSON into a DiagramDoc. Returns error string or null. */
export function validateDoc(raw: unknown): string | null {
  if (typeof raw !== "object" || raw === null) return "Not a JSON object.";
  const d = raw as Record<string, unknown>;
  if (d["version"] !== DIAGRAM_VERSION)
    return `Unsupported diagram version (got ${String(d["version"])}, need ${DIAGRAM_VERSION}).`;
  if (!Array.isArray(d["elements"])) return "Missing elements array.";
  if (!Array.isArray(d["connections"])) return "Missing connections array.";
  if (typeof d["viewport"] !== "object" || d["viewport"] === null)
    return "Missing viewport.";
  for (const [i, el] of (d["elements"] as unknown[]).entries()) {
    if (typeof el !== "object" || el === null) return `Element ${i} is not an object.`;
    const e = el as Record<string, unknown>;
    for (const k of ["id", "type", "x", "y", "w", "h", "label"] as const) {
      if (!(k in e)) return `Element ${i} missing "${k}".`;
    }
    if (!Number.isFinite(e["x"]) || !Number.isFinite(e["y"]))
      return `Element ${i} has non-numeric position.`;
  }
  for (const [i, c] of (d["connections"] as unknown[]).entries()) {
    if (typeof c !== "object" || c === null) return `Connection ${i} is not an object.`;
    const cc = c as Record<string, unknown>;
    if (typeof cc["id"] !== "string") return `Connection ${i} missing id.`;
    for (const end of ["source", "target"] as const) {
      const s = cc[end] as Record<string, unknown> | undefined;
      if (typeof s?.["node"] !== "string" || typeof s?.["port"] !== "string")
        return `Connection ${i} has bad ${end}.`;
    }
  }
  return null;
}
