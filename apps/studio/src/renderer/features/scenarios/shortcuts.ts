export type CanvasCommand =
  "delete" | "duplicate" | "selectAll" | "deselect" | "undo" | "redo" | "save";

export interface KeyPress {
  readonly key: string;
  readonly ctrlKey: boolean;
  readonly metaKey: boolean;
  readonly shiftKey: boolean;
  readonly altKey: boolean;
}

export function canvasCommand(press: KeyPress): CanvasCommand | null {
  const mod = press.ctrlKey || press.metaKey;
  const key = press.key.toLowerCase();
  if (press.altKey) return null;
  if (!mod && (key === "delete" || key === "backspace")) return "delete";
  if (!mod && key === "escape") return "deselect";
  if (!mod) return null;
  if (key === "s" && !press.shiftKey) return "save";
  if (key === "z") return press.shiftKey ? "redo" : "undo";
  if (key === "y" && !press.shiftKey) return "redo";
  if (key === "d" && !press.shiftKey) return "duplicate";
  if (key === "a" && !press.shiftKey) return "selectAll";
  return null;
}

export function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}
