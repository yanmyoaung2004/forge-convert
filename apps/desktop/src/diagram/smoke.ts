// DOM-free smoke tests for the SVG diagram core (run with bun).
// Covers: IDs, registry, geometry, routing, history, layout, serialize,
// validation, templates. No Vue, no Tauri, no filesystem.
import { blankDoc, newId, validateDoc } from "./types";
import { ELEMENTS, createElement, baseOf, diamondPoints } from "./elements";
import { screenToWorld, portPoint, connectionPath, dashArray, hitElement } from "./geometry";
import { toSvg, contentBounds } from "./serialize";
import { History } from "./history";
import { layoutVertical, layoutHorizontal, layoutHierarchy } from "./layout";
import { TEMPLATES } from "./templates";
import type { DiagramDoc } from "./types";

let pass = 0;
let fail = 0;
function check(name: string, cond: boolean): void {
  if (cond) {
    pass += 1;
  } else {
    fail += 1;
    console.error(`FAIL: ${name}`);
  }
}

// IDs unique + prefixed
const ids = new Set([newId("el"), newId("el"), newId("conn")]);
check("ids unique", ids.size === 3);

// Registry: 18 kinds, developer types present
check("registry has 18", ELEMENTS.length === 18);
check("registry has api/db", ELEMENTS.some((e) => e.kind === "api") && ELEMENTS.some((e) => e.kind === "database"));
check("baseOf api is rounded", baseOf("api") === "rounded");
check("diamond points", diamondPoints(0, 0, 100, 60).split(" ").length === 4);

// Geometry: screen→world at zoom 2 with pan
const w = screenToWorld(100, 50, { left: 0, top: 0 } as DOMRect, { panX: 20, panY: 10, zoom: 2 });
check("screenToWorld", w.x === 40 && w.y === 20);

// Ports on borders
const el = createElement("rect", 100, 100);
check("port n", portPoint(el, "n").y === el.y);
check("port e", portPoint(el, "e").x === el.x + el.w);
check("hit inside", hitElement(el, { x: el.x + 5, y: el.y + 5 }));
check("hit outside", !hitElement(el, { x: el.x - 5, y: el.y - 5 }));

// Routing: straight / orthogonal / curved
const doc: DiagramDoc = blankDoc();
const a = createElement("rounded", 100, 100);
a.label = "A";
const b = createElement("rounded", 400, 300);
b.label = "B";
doc.elements.push(a, b);
const straight = { id: "c1", source: { node: a.id, port: "e" as const }, target: { node: b.id, port: "w" as const }, kind: "straight" as const, color: "#000", width: 2, dash: "solid" as const, arrow: "end" as const, opacity: 1 };
check("straight path", connectionPath(straight, doc.elements).startsWith("M "));
const ortho = { ...straight, id: "c2", kind: "orthogonal" as const };
check("orthogonal has elbow", (connectionPath(ortho, doc.elements).match(/L/g) ?? []).length === 2);
const curved = { ...straight, id: "c3", kind: "curved" as const };
check("curved has cubic", connectionPath(curved, doc.elements).includes("C "));
check("dash solid empty", dashArray(straight) === "");
check("dash dashed", dashArray({ dash: "dashed" }) === "8 5");
check("dash dotted", dashArray({ dash: "dotted" }) === "2 4");
check("missing node empty path", connectionPath(straight, []) === "");

// History: commit → undo → redo
const h = new History(doc);
check("no undo initially", !h.canUndo());
doc.elements.push(createElement("circle", 0, 0));
h.commit(doc);
check("can undo after commit", h.canUndo());
const undone = h.undo();
check("undo drops element", undone !== null && undone.elements.length === 2);
check("can redo", h.canRedo());
const redone = h.redo();
check("redo restores", redone !== null && redone.elements.length === 3);

// Layout: vertical orders by depth
const lay: DiagramDoc = blankDoc();
const n1 = createElement("rect", 0, 0); n1.label = "1";
const n2 = createElement("rect", 0, 0); n2.label = "2";
const n3 = createElement("rect", 0, 0); n3.label = "3";
lay.elements.push(n1, n2, n3);
lay.connections.push(
  { id: "l1", source: { node: n1.id, port: "s" }, target: { node: n2.id, port: "n" }, kind: "straight", color: "#000", width: 2, dash: "solid", arrow: "end", opacity: 1 },
  { id: "l2", source: { node: n2.id, port: "s" }, target: { node: n3.id, port: "n" }, kind: "straight", color: "#000", width: 2, dash: "solid", arrow: "end", opacity: 1 },
);
layoutVertical(lay.elements, lay.connections);
check("vertical orders top-down", n1.y < n2.y && n2.y < n3.y);
layoutHorizontal(lay.elements, lay.connections);
check("horizontal orders left-right", n1.x < n2.x && n2.x < n3.x);
layoutHierarchy(lay.elements, lay.connections);
check("hierarchy orders top-down", n1.y < n3.y);

// Serialize: valid SVG, viewBox, labels, escaping
const svg = toSvg(doc);
check("svg root", svg.startsWith("<svg viewBox="));
check("svg xmlns", svg.includes('xmlns="http://www.w3.org/2000/svg"'));
check("svg has labels", svg.includes("A") && svg.includes("B"));
const evil: DiagramDoc = blankDoc();
const t = createElement("text", 0, 0);
t.label = "<b>&\"quoted\"</b>";
evil.elements.push(t);
check("svg escapes", toSvg(evil).includes("&lt;b&gt;") && !toSvg(evil).includes("<b>"));
check("empty bounds default", contentBounds(blankDoc()).w === 800);

// Validation
check("blank valid", validateDoc(blankDoc()) === null);
check("bad version", validateDoc({ ...blankDoc(), version: 99 }) !== null);
check("missing elements", validateDoc({ version: 1, connections: [], viewport: { x: 0, y: 0, zoom: 1 } }) !== null);

// Templates: all build valid docs
for (const t of TEMPLATES) {
  const d = t.build();
  check(`template ${t.key} valid`, validateDoc(d) === null && d.elements.length > 0);
}
check("7 templates", TEMPLATES.length === 7);

console.log(`\n${pass} passed, ${fail} failed`);
if (fail > 0) throw new Error(`${fail} smoke checks failed`);
