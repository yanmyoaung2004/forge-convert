// Starting-point templates: each builds an editable DiagramDoc.
// Same model as hand-made diagrams; every element stays editable.
import { blankDoc, newId, type DiagramDoc, type DiagramElement } from "./types";
import { createElement } from "./elements";
import { layoutVertical } from "./layout";

export interface Template {
  key: string;
  name: string;
  build(): DiagramDoc;
}

function box(
  doc: DiagramDoc,
  kind: Parameters<typeof createElement>[0],
  x: number,
  y: number,
  label: string,
): DiagramElement {
  const el = { ...createElement(kind, x, y), label };
  doc.elements.push(el);
  return el;
}

function link(
  doc: DiagramDoc,
  a: DiagramElement,
  b: DiagramElement,
  aPort: "n" | "e" | "s" | "w" = "s",
  bPort: "n" | "e" | "s" | "w" = "n",
): void {
  doc.connections.push({
    id: newId("conn"),
    source: { node: a.id, port: aPort },
    target: { node: b.id, port: bPort },
    kind: "curved",
    color: "#1c1917",
    width: 2,
    dash: "solid",
    arrow: "end",
    opacity: 1,
  });
}

function flow(labels: string[], kinds?: Array<Parameters<typeof createElement>[0]>): DiagramDoc {
  const doc = blankDoc();
  let prev: DiagramElement | undefined;
  labels.forEach((label, i) => {
    const el = box(doc, kinds?.[i] ?? "rounded", 200, 80 + i * 140, label);
    if (prev) link(doc, prev, el);
    prev = el;
  });
  layoutVertical(doc.elements, doc.connections);
  return doc;
}

export const TEMPLATES: Template[] = [
  { key: "flow", name: "Basic Flowchart", build: () => flow(["Start", "Process", "Decision", "End"], ["rounded", "process", "diamond", "rounded"]) },
  {
    key: "software", name: "Software Architecture",
    build: () => {
      const doc = blankDoc("Software Architecture");
      const client = box(doc, "client", 200, 80, "Client");
      const api = box(doc, "api", 200, 220, "API");
      const db = box(doc, "database", 200, 360, "Database");
      link(doc, client, api);
      link(doc, api, db);
      layoutVertical(doc.elements, doc.connections);
      return doc;
    },
  },
  {
    key: "api", name: "API Architecture",
    build: () => flow(["Client", "Gateway", "Service", "Database"], ["client", "api", "server", "database"]),
  },
  {
    key: "agent", name: "AI Agent Architecture",
    build: () => flow(["Input", "Agent", "Tools", "Output"], ["input", "process", "server", "output"]),
  },
  {
    key: "pipeline", name: "Input/Output Pipeline",
    build: () => flow(["Source", "Queue", "Worker", "Storage"], ["input", "queue", "process", "storage"]),
  },
  {
    key: "hierarchy", name: "Simple Hierarchy",
    build: () => {
      const doc = blankDoc("Simple Hierarchy");
      const root = box(doc, "rounded", 300, 80, "Parent");
      const a = box(doc, "rounded", 150, 220, "Child A");
      const b = box(doc, "rounded", 450, 220, "Child B");
      link(doc, root, a);
      link(doc, root, b);
      layoutVertical(doc.elements, doc.connections);
      return doc;
    },
  },
  {
    key: "db", name: "Database Architecture",
    build: () => flow(["App", "Cache", "Database"], ["server", "queue", "database"]),
  },
];
