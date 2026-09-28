// SVG import: pasted/file SVG → editable DiagramDoc.
// Inverse of serialize.ts (lossy by design): vector shapes become elements,
// paths/text reconnect as editable nodes. Layout/positions preserved.
// DOMParser-based (browser/WebView2 native, no dependency). Pure + testable:
// parsing takes a string, never touches the filesystem or Vue.
import { blankDoc, newId, type DiagramConnection, type DiagramDoc, type DiagramElement, type ElementKind, type Port } from "./types";
import { ELEMENTS } from "./elements";

export interface ImportResult {
  doc: DiagramDoc;
  /** Human-readable notes: what mapped, what was skipped, what to check. */
  notes: string[];
  /** True when nothing usable was found (caller shows error, keeps old doc). */
  empty: boolean;
}

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
  fill: string;
  stroke: string;
  strokeWidth: number;
  opacity: number;
  rx: number;
}

function attr(e: XmlEl, name: string, fallback = ""): string {
  return e.attr(name) ?? fallback;
}

function num(v: string, fallback = 0): number {
  const n = Number.parseFloat(v);
  return Number.isFinite(n) ? n : fallback;
}

/**
 * Minimal XML element for SVG import (no DOM dependency: works in browser,
 * WebView2, AND bun smoke tests). Regex-based, handles flat + one-level
 * nested elements (our serializer emits flat children; foreign SVGs with
 * deep nesting still yield their leaf shapes via recursive descent).
 */
interface XmlEl {
  tag: string;
  attrs: Record<string, string>;
  text: string;
  attr(name: string): string | null;
}
function parseAttrs(raw: string): Record<string, string> {
  const attrs: Record<string, string> = {};
  const re = /([\w:-]+)\s*=\s*("([^"]*)"|'([^']*)')/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(raw)) !== null) attrs[m[1]] = m[3] ?? m[4] ?? "";
  return attrs;
}
function parseSvgXml(input: string): XmlEl[] {
  // Strip prolog/comments/doctype; collect every element open (self-close
  // or paired). Text content captured for <text> leaves.
  const src = input
    .replace(/^<\?xml[^?]*\?>\s*/, "")
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<!DOCTYPE[^>]*>/g, "");
  const els: XmlEl[] = [];
  // Paired elements first (text, g, svg containers).
  const paired = /<(\w+)([^>]*?)>([\s\S]*?)<\/\1\s*>/g;
  let m: RegExpExecArray | null;
  const consumed = new Set<number>();
  while ((m = paired.exec(src)) !== null) {
    const [, tag, rawAttrs, inner] = m;
    if (tag === "svg" || tag === "g" || tag === "defs" || tag === "marker") {
      // Recurse into containers (flatten one level; loop handles depth).
      for (const sub of parseSvgXml(inner)) els.push(sub);
      continue;
    }
    const attrs = parseAttrs(rawAttrs);
    els.push({ tag, attrs, text: inner.replace(/<[^>]*>/g, "").trim(), attr(name: string) { return attrs[name] ?? null; } });
    consumed.add(m.index);
  }
  // Self-closing + void elements (rect/circle/line/path/...).
  const selfClose = /<(\w+)([^>]*?)\/>/g;
  while ((m = selfClose.exec(src)) !== null) {
    const [, tag, rawAttrs] = m;
    if (tag === "svg" || tag === "g" || tag === "defs" || tag === "marker") continue;
    const attrs = parseAttrs(rawAttrs);
    els.push({ tag, attrs, text: "", attr(name: string) { return attrs[name] ?? null; } });
  }
  return els;
}
function parseFill(e: XmlEl): { fill: string; stroke: string; strokeWidth: number; opacity: number } {
  const style = attr(e, "style");
  const pick = (names: string[]): string | null => {
    for (const n of names) {
      const m = new RegExp(`${n}\\s*:\\s*([^;]+)`).exec(style);
      if (m) return m[1].trim();
    }
    return null;
  };
  const fill = attr(e, "fill", pick(["fill"]) ?? "#ffffff");
  const stroke = attr(e, "stroke", pick(["stroke"]) ?? "#1c1917");
  const sw = attr(e, "stroke-width", pick(["stroke-width"]) ?? "2");
  const op = attr(e, "opacity", pick(["opacity"]) ?? "1");
  return {
    fill: fill === "none" ? "#ffffff" : fill,
    stroke: stroke === "none" ? "#1c1917" : stroke,
    strokeWidth: Math.min(12, Math.max(0, num(sw, 2))),
    opacity: Math.min(1, Math.max(0.1, num(op, 1))),
  };
}

/** Rough shape classifier for rect-likes (rounded vs square). */
function rectKind(rx: number): ElementKind {
  return rx > 2 ? "rounded" : "rect";
}

function makeEl(kind: ElementKind, box: Box, label: string, extra?: Partial<DiagramElement>): DiagramElement {
  const def = ELEMENTS.find((d) => d.kind === kind);
  return {
    id: newId("el"),
    type: kind,
    x: Math.round(box.x),
    y: Math.round(box.y),
    w: Math.max(30, Math.round(box.w)),
    h: Math.max(30, Math.round(box.h)),
    label,
    fill: box.fill,
    stroke: box.stroke,
    strokeWidth: box.strokeWidth,
    opacity: box.opacity,
    radius: def?.radius,
    fontSize: def?.fontSize ?? 14,
    fontWeight: 400,
    textColor: "#1c1917",
    align: "center",
    ...extra,
  };
}

/** Parse `points="x,y x,y ..."` polygons (diamonds). */
function parsePoints(raw: string): Array<{ x: number; y: number }> {
  return raw
    .trim()
    .split(/[\s,]+/)
    .map(Number)
    .reduce<Array<{ x: number; y: number }>>((acc, n, i, arr) => {
      if (i % 2 === 0 && Number.isFinite(n) && Number.isFinite(arr[i + 1])) acc.push({ x: n, y: arr[i + 1] });
      return acc;
    }, []);
}

/** Path `d` bounding box (M/L/H/V/C/Q/A/Z + numbers). Curves approximated by endpoints. */
function pathBounds(d: string): Box | null {
  const tokens = d.match(/[MLHVCAQZmlhvcaqz]|-?\d*\.?\d+(?:e-?\d+)?/g);
  if (!tokens) return null;
  let x = 0;
  let y = 0;
  let sx = 0;
  let sy = 0;
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  const dot = (px: number, py: number): void => {
    x0 = Math.min(x0, px);
    y0 = Math.min(y0, py);
    x1 = Math.max(x1, px);
    y1 = Math.max(y1, py);
  };
  let i = 0;
  let cmd = "";
  const take = (): number => num(tokens[i++] ?? "0");
  while (i < tokens.length) {
    const t = tokens[i++];
    if (/^[MLHVCAQZmlhvcaqz]$/.test(t)) {
      cmd = t;
      if (/^[Zz]$/.test(cmd)) {
        x = sx;
        y = sy;
      }
      continue;
    }
    i -= 1;
    const rel = cmd === cmd.toLowerCase() && cmd !== "";
    const U = cmd.toUpperCase();
    if (U === "M" || U === "L") {
      const nx = take();
      const ny = take();
      x = rel ? x + nx : nx;
      y = rel ? y + ny : ny;
      if (U === "M") {
        sx = x;
        sy = y;
      }
      dot(x, y);
      if (U === "M") cmd = rel ? "l" : "L";
    } else if (U === "H") {
      const nx = take();
      x = rel ? x + nx : nx;
      dot(x, y);
    } else if (U === "V") {
      const ny = take();
      y = rel ? y + ny : ny;
      dot(x, y);
    } else if (U === "C") {
      const pts: number[] = [take(), take(), take(), take(), take(), take()];
      x = rel ? x + pts[4] : pts[4];
      y = rel ? y + pts[5] : pts[5];
      dot(x, y);
    } else if (U === "Q") {
      const pts: number[] = [take(), take(), take(), take()];
      x = rel ? x + pts[2] : pts[2];
      y = rel ? y + pts[3] : pts[3];
      dot(x, y);
    } else if (U === "A") {
      const rx = take();
      const ry = take();
      take();
      take();
      take();
      const nx = take();
      const ny = take();
      x = rel ? x + nx : nx;
      y = rel ? y + ny : ny;
      dot(x, y);
      void rx;
      void ry;
    } else {
      i += 1;
    }
  }
  if (!Number.isFinite(x0) || x1 - x0 < 2 || y1 - y0 < 2) return null;
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0, fill: "#ffffff", stroke: "#1c1917", strokeWidth: 2, opacity: 1, rx: 0 };
}

function nearestPort(a: { x: number; y: number }, b: { x: number; y: number }): { from: Port; to: Port } {
  // Endpoint near top/bottom edge → n/s, else e/w. Deterministic, no guessing.
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const from: Port = Math.abs(dx) > Math.abs(dy) ? (dx >= 0 ? "e" : "w") : dy >= 0 ? "s" : "n";
  const to: Port = Math.abs(dx) > Math.abs(dy) ? (dx >= 0 ? "w" : "e") : dy >= 0 ? "n" : "s";
  return { from, to };
}

/**
 * Parse an SVG string into an editable document.
 * Maps: rect/rounded/circle/ellipse/polygon/diamond, line/polyline,
 * path (M/L/H/V/C/Q/A/Z → rect or diamond-by-points, else generic path
 * node), text (attached to nearest shape bbox, else standalone), groups
 * flattened via getElementsByTagName (nested <g transform> unsupported —
 * reported in notes; flatten transforms before import for best results).
 */
export function svgToDoc(svgText: string, name = "Imported SVG"): ImportResult {
  const notes: string[] = [];
  const doc = blankDoc(name);
  const connections: DiagramConnection[] = [];
  const cleaned = svgText.trim();
  if (!cleaned) return { doc, notes: ["Empty input — nothing imported."], empty: true };
  if (!cleaned.includes("<svg")) return { doc, notes: ["No <svg> root found — paste full SVG markup."], empty: true };
  const els = parseSvgXml(cleaned);
  if (els.length === 0) return { doc, notes: ["Could not parse SVG (no elements found — check the markup)."], empty: true };
  const hasTransform = /<g[^>]*transform/.test(cleaned);
  if (hasTransform) notes.push("Nested <g transform> is flattened without transform math — flatten transforms first for exact positions.");

  const usedText = new Set<XmlEl>();

  const claimText = (box: Box): { label: string; size: number; weight: number; color: string } => {
    const texts = els.filter((t) => t.tag === "text" && !usedText.has(t));
    let best: XmlEl | null = null;
    let bestLen = 0;
    for (const t of texts) {
      const tx = num(attr(t, "x", "0"));
      const ty = num(attr(t, "y", "0"));
      if (tx >= box.x - 4 && tx <= box.x + box.w + 4 && ty >= box.y - 20 && ty <= box.y + box.h + 20) {
        const len = t.text.trim().length;
        if (len > bestLen) {
          bestLen = len;
          best = t;
        }
      }
    }
    if (best) {
      usedText.add(best);
      return {
        label: best.text.trim(),
        size: Math.min(48, Math.max(8, Math.round(num(attr(best, "font-size", "14"))))),
        weight: num(attr(best, "font-weight", "400")) >= 600 ? 600 : 400,
        color: attr(best, "fill", "#1c1917"),
      };
    }
    return { label: "", size: 14, weight: 400, color: "#1c1917" };
  };

  // rect (+ rounded via rx)
  for (const r of els.filter((e) => e.tag === "rect")) {
    const w = num(attr(r, "width"));
    const h = num(attr(r, "height"));
    if (w < 4 || h < 4) continue;
    const box: Box = {
      x: num(attr(r, "x")), y: num(attr(r, "y")), w, h,
      ...parseFill(r), rx: num(attr(r, "rx")) || num(attr(r, "ry")),
    };
    const t = claimText(box);
    const el = makeEl(rectKind(box.rx), box, t.label || "Rect", {
      fontSize: t.size, fontWeight: t.weight, textColor: t.color,
      radius: box.rx > 2 ? Math.round(box.rx) : undefined,
    });
    doc.elements.push(el);
  }
  // circle
  for (const c of els.filter((e) => e.tag === "circle")) {
    const rr = num(attr(c, "r"));
    if (rr < 2) continue;
    const cx = num(attr(c, "cx"));
    const cy = num(attr(c, "cy"));
    const box: Box = { x: cx - rr, y: cy - rr, w: rr * 2, h: rr * 2, ...parseFill(c), rx: 0 };
    const t = claimText(box);
    const el = makeEl("circle", box, t.label || "Circle", { fontSize: t.size, fontWeight: t.weight, textColor: t.color });
    doc.elements.push(el);
  }
  // ellipse
  for (const e of els.filter((e) => e.tag === "ellipse")) {
    const rx = num(attr(e, "rx"));
    const ry = num(attr(e, "ry"));
    if (rx < 2 || ry < 2) continue;
    const box: Box = { x: num(attr(e, "cx")) - rx, y: num(attr(e, "cy")) - ry, w: rx * 2, h: ry * 2, ...parseFill(e), rx: 0 };
    const t = claimText(box);
    const el = makeEl("ellipse", box, t.label || "Ellipse", { fontSize: t.size, fontWeight: t.weight, textColor: t.color });
    doc.elements.push(el);
  }
  // polygon / polyline (diamond by 4 points, else bbox rect)
  for (const p of els.filter((e) => e.tag === "polygon" || e.tag === "polyline")) {
    const pts = parsePoints(attr(p, "points"));
    if (pts.length < 3) continue;
    const xs = pts.map((q) => q.x);
    const ys = pts.map((q) => q.y);
    const box: Box = {
      x: Math.min(...xs), y: Math.min(...ys),
      w: Math.max(...xs) - Math.min(...xs), h: Math.max(...ys) - Math.min(...ys),
      ...parseFill(p), rx: 0,
    };
    if (box.w < 4 || box.h < 4) continue;
    const t = claimText(box);
    const isDiamond =
      pts.length === 4 &&
      Math.abs(box.w / 2 - (pts[0].x - box.x)) < box.w * 0.2 &&
      Math.abs(box.h / 2 - (pts[1].y - box.y)) < box.h * 0.2;
    const el = makeEl(isDiamond ? "diamond" : "rect", box, t.label || (isDiamond ? "Diamond" : "Shape"), {
      fontSize: t.size, fontWeight: t.weight, textColor: t.color,
    });
    doc.elements.push(el);
  }
  // line → straight connection between nearest elements (or standalone note)
  const orphanLines: string[] = [];
  for (const l of els.filter((e) => e.tag === "line")) {
    const a = { x: num(attr(l, "x1")), y: num(attr(l, "y1")) };
    const b = { x: num(attr(l, "x2")), y: num(attr(l, "y2")) };
    if (Math.hypot(b.x - a.x, b.y - a.y) < 4) continue;
    const st = parseFill(l);
    const owner = (p: { x: number; y: number }): DiagramElement | null => {
      let bestEl: DiagramElement | null = null;
      let bestD = 30;
      for (const el of doc.elements) {
        const cx = Math.min(Math.max(p.x, el.x), el.x + el.w);
        const cy = Math.min(Math.max(p.y, el.y), el.y + el.h);
        const d = Math.hypot(p.x - cx, p.y - cy);
        if (d < bestD) {
          bestD = d;
          bestEl = el;
        }
      }
      return bestEl;
    };
    const ea = owner(a);
    const eb = owner(b);
    if (ea && eb && ea.id !== eb.id) {
      const ports = nearestPort(a, b);
      connections.push({
        id: newId("conn"), source: { node: ea.id, port: ports.from }, target: { node: eb.id, port: ports.to },
        kind: "straight", color: st.stroke === "#1c1917" && attr(l, "stroke", "") === "" ? "#1c1917" : st.stroke,
        width: Math.min(8, Math.max(1, Math.round(num(attr(l, "stroke-width", "2"))))),
        dash: attr(l, "stroke-dasharray", "").includes(",") || num(attr(l, "stroke-dasharray", "0")) > 0 ? "dashed" : "solid",
        arrow: "none", opacity: st.opacity,
      });
    } else {
      orphanLines.push(`line (${Math.round(a.x)},${Math.round(a.y)})→(${Math.round(b.x)},${Math.round(b.y)})`);
    }
  }
  // path → bbox node (marker-aware: marker-start/end → arrow style)
  let pathCount = 0;
  for (const p of els.filter((e) => e.tag === "path")) {
    const d = attr(p, "d");
    if (!d) continue;
    const bb = pathBounds(d);
    if (!bb) continue;
    const st = parseFill(p);
    const hasStart = attr(p, "marker-start", "") !== "";
    const hasEnd = attr(p, "marker-end", "") !== "";
    const looksEdge = hasStart || hasEnd || /[CcQq]/.test(d);
    if (looksEdge && bb.w > 30 && bb.h > 8) {
      // Bezier edge → connection between nearest elements at endpoints.
      const nums = (d.match(/-?\d*\.?\d+(?:e-?\d+)?/g) ?? []).map(Number);
      const end = nums.length >= 4 ? { x: nums[nums.length - 2], y: nums[nums.length - 1] } : { x: bb.x + bb.w, y: bb.y + bb.h / 2 };
      const startM = d.match(/M\s*(-?\d*\.?\d+)[,\s]+(-?\d*\.?\d+)/);
      const start = startM ? { x: Number(startM[1]), y: Number(startM[2]) } : { x: bb.x, y: bb.y + bb.h / 2 };
      const owner = (pt: { x: number; y: number }): DiagramElement | null => {
        let bestEl: DiagramElement | null = null;
        let bestD = 40;
        for (const el of doc.elements) {
          const cx = Math.min(Math.max(pt.x, el.x), el.x + el.w);
          const cy = Math.min(Math.max(pt.y, el.y), el.y + el.h);
          const dd = Math.hypot(pt.x - cx, pt.y - cy);
          if (dd < bestD) {
            bestD = dd;
            bestEl = el;
          }
        }
        return bestEl;
      };
      const ea = owner(start);
      const eb = owner(end);
      if (ea && eb && ea.id !== eb.id) {
        const ports = nearestPort(start, end);
        connections.push({
          id: newId("conn"), source: { node: ea.id, port: ports.from }, target: { node: eb.id, port: ports.to },
          kind: /[CcQq]/.test(d) ? "curved" : "straight",
          color: st.stroke, width: Math.min(8, Math.max(1, Math.round(num(attr(p, "stroke-width", "2"))))),
          dash: attr(p, "stroke-dasharray", "") ? "dashed" : "solid",
          arrow: hasStart && hasEnd ? "both" : hasEnd ? "end" : hasStart ? "start" : "end",
          opacity: st.opacity,
        });
        continue;
      }
    }
    pathCount += 1;
    const t = claimText({ ...bb, ...st, rx: 0 });
    const el = makeEl("rounded", { ...bb, ...st, rx: 8 }, t.label || `Path ${pathCount}`, {
      fontSize: t.size, fontWeight: t.weight, textColor: t.color, radius: 8,
    });
    doc.elements.push(el);
  }
  // standalone text (not claimed by any shape)
  for (const t of els.filter((e) => e.tag === "text")) {
    if (usedText.has(t)) continue;
    const content = t.text.trim();
    if (!content) continue;
    const tx = num(attr(t, "x", "0"));
    const ty = num(attr(t, "y", "0"));
    const size = Math.min(48, Math.max(8, Math.round(num(attr(t, "font-size", "14")))));
    doc.elements.push(makeEl("text", {
      x: tx - 80, y: ty - size, w: 160, h: size + 16,
      fill: "#ffffff", stroke: "#1c1917", strokeWidth: 0, opacity: 1, rx: 0,
    }, content, { fontSize: size, textColor: attr(t, "fill", "#1c1917") }));
  }

  if (orphanLines.length > 0) notes.push(`${orphanLines.length} line(s) had no nearby shapes — skipped (first: ${orphanLines[0]}).`);
  if (pathCount > 0) notes.push(`${pathCount} freeform path(s) imported as rounded boxes (exact curves not preserved).`);
  if (doc.elements.length === 0 && connections.length === 0)
    return { doc, notes: ["No importable shapes found (need rect/circle/ellipse/polygon/path/text)."], empty: true };

  // Deterministic connection kind: keep straight unless clearly curved.
  doc.connections.push(...connections);
  doc.viewport = { x: 0, y: 0, zoom: 1 };
  doc.metadata.updatedAt = new Date().toISOString();
  notes.unshift(`Imported ${doc.elements.length} element(s), ${connections.length} connection(s). Positions preserved; review arrows + labels.`);
  return { doc, notes, empty: false };
}
