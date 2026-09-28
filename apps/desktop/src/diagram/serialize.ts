// SVG serialization: DiagramDoc → standalone vector SVG string.
// No Vue/DOM. Escapes text, emits markers once, viewBox from content bounds.
import type { DiagramConnection, DiagramDoc, DiagramElement } from "./types";
import { baseOf, cylinderPaths, diamondPoints } from "./elements";
import { connectionPath, dashArray, portPoint } from "./geometry";

function esc(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function num(n: number): string {
  return String(Math.round(n * 10) / 10);
}

/** Content bounds of all elements (+ padding) for the viewBox. */
export function contentBounds(doc: DiagramDoc, pad = 40): { x: number; y: number; w: number; h: number } {
  if (doc.elements.length === 0) return { x: 0, y: 0, w: 800, h: 600 };
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const el of doc.elements) {
    x0 = Math.min(x0, el.x);
    y0 = Math.min(y0, el.y);
    x1 = Math.max(x1, el.x + el.w);
    y1 = Math.max(y1, el.y + el.h);
  }
  return { x: x0 - pad, y: y0 - pad, w: x1 - x0 + pad * 2, h: y1 - y0 + pad * 2 };
}

function shapeSvg(el: DiagramElement): string {
  const common = `fill="${el.fill}" stroke="${el.stroke}" stroke-width="${el.strokeWidth}" opacity="${el.opacity}"`;
  const base = baseOf(el.type);
  if (base === "circle") {
    const r = Math.min(el.w, el.h) / 2;
    return `<circle cx="${num(el.x + el.w / 2)}" cy="${num(el.y + el.h / 2)}" r="${num(r)}" ${common}/>`;
  }
  if (base === "ellipse") {
    return `<ellipse cx="${num(el.x + el.w / 2)}" cy="${num(el.y + el.h / 2)}" rx="${num(el.w / 2)}" ry="${num(el.h / 2)}" ${common}/>`;
  }
  if (base === "diamond") {
    return `<polygon points="${diamondPoints(el.x, el.y, el.w, el.h)}" ${common}/>`;
  }
  if (base === "cylinder") {
    const { body, top } = cylinderPaths(el.x, el.y, el.w, el.h);
    return `<path d="${body}" ${common}/><path d="${top}" ${common}/>`;
  }
  if (base === "text") return "";
  if (base === "rounded") {
    const rad = el.radius ?? 10;
    return `<rect x="${num(el.x)}" y="${num(el.y)}" width="${num(el.w)}" height="${num(el.h)}" rx="${num(rad)}" ${common}/>`;
  }
  return `<rect x="${num(el.x)}" y="${num(el.y)}" width="${num(el.w)}" height="${num(el.h)}" ${common}/>`;
}

function textSvg(el: DiagramElement): string {
  const lines = el.label.split("\n");
  const anchor = el.align === "left" ? "start" : el.align === "right" ? "end" : "middle";
  const tx = el.align === "left" ? el.x + 8 : el.align === "right" ? el.x + el.w - 8 : el.x + el.w / 2;
  const lh = el.fontSize * 1.25;
  const startY = el.y + el.h / 2 - ((lines.length - 1) * lh) / 2;
  const tspans = lines
    .map(
      (ln, i) =>
        `<tspan x="${num(tx)}" dy="${i === 0 ? num(-((lines.length - 1) * lh) / 2) : num(lh)}">${esc(ln)}</tspan>`,
    )
    .join("");
  void startY;
  return `<text x="${num(tx)}" y="${num(el.y + el.h / 2)}" text-anchor="${anchor}" font-size="${el.fontSize}" font-weight="${el.fontWeight}" fill="${el.textColor}" font-family="Segoe UI, system-ui, sans-serif">${tspans}</text>`;
}

function connSvg(conn: DiagramConnection, elements: DiagramElement[]): string {
  const d = connectionPath(conn, elements);
  if (!d) return "";
  const dash = dashArray(conn);
  const markerStart = conn.arrow === "start" || conn.arrow === "both" ? ` marker-start="url(#arr)"` : "";
  const markerEnd = conn.arrow === "end" || conn.arrow === "both" ? ` marker-end="url(#arr)"` : "";
  return `<path d="${d}" fill="none" stroke="${conn.color}" stroke-width="${conn.width}"${dash ? ` stroke-dasharray="${dash}"` : ""} opacity="${conn.opacity}"${markerStart}${markerEnd}/>`;
}

/** Full standalone SVG document string. Valid, compact, embeddable. */
export function toSvg(doc: DiagramDoc): string {
  const b = contentBounds(doc);
  const shapes = doc.elements.map(shapeSvg).filter(Boolean).join("");
  const texts = doc.elements.map(textSvg).join("");
  const conns = doc.connections.map((c) => connSvg(c, doc.elements)).join("");
  void portPoint;
  return (
    `<svg viewBox="${num(b.x)} ${num(b.y)} ${num(b.w)} ${num(b.h)}" xmlns="http://www.w3.org/2000/svg" role="img">` +
    `<defs><marker id="arr" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M 0 1 L 9 5 L 0 9 z" fill="context-stroke"/></marker></defs>` +
    `${conns}${shapes}${texts}</svg>`
  );
}
