// Snapshot history: simple, reliable undo/redo for ALL ops.
// Diagrams are small; a JSON snapshot per commit beats command objects.
import type { DiagramDoc } from "./types";

const CAP = 100;

export class History {
  private past: string[] = [];
  private future: string[] = [];
  private current: string;

  constructor(initial: DiagramDoc) {
    this.current = JSON.stringify(initial);
  }

  /** Commit current doc AFTER a mutation. Clears redo. */
  commit(doc: DiagramDoc): void {
    this.past.push(this.current);
    if (this.past.length > CAP) this.past.shift();
    this.current = JSON.stringify(doc);
    this.future = [];
  }

  canUndo(): boolean {
    return this.past.length > 0;
  }

  canRedo(): boolean {
    return this.future.length > 0;
  }

  undo(): DiagramDoc | null {
    const prev = this.past.pop();
    if (prev === undefined) return null;
    this.future.push(this.current);
    this.current = prev;
    return JSON.parse(prev) as DiagramDoc;
  }

  redo(): DiagramDoc | null {
    const next = this.future.pop();
    if (next === undefined) return null;
    this.past.push(this.current);
    this.current = next;
    return JSON.parse(next) as DiagramDoc;
  }

  /** Reset after load/new (fresh baseline, no undo past). */
  reset(doc: DiagramDoc): void {
    this.past = [];
    this.future = [];
    this.current = JSON.stringify(doc);
  }
}
