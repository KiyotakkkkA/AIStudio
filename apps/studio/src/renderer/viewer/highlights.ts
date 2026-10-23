export const MATCH_HIGHLIGHT = "zvs-viewer-match";
export const CURRENT_HIGHLIGHT = "zvs-viewer-current";
export const PASSAGE_HIGHLIGHT = "zvs-viewer-passage";

interface HighlightRegistry {
  set(name: string, highlight: unknown): void;
  delete(name: string): void;
}

type HighlightConstructor = new (...ranges: Range[]) => unknown;

function registry(): {
  readonly highlights: HighlightRegistry;
  readonly Highlight: HighlightConstructor;
} | null {
  const css = (globalThis as { CSS?: { highlights?: HighlightRegistry } }).CSS;
  const Highlight = (globalThis as { Highlight?: HighlightConstructor }).Highlight;
  if (css?.highlights === undefined || Highlight === undefined) return null;
  return { highlights: css.highlights, Highlight };
}

export function paint(name: string, ranges: readonly Range[]): void {
  const found = registry();
  if (found === null) return;
  if (ranges.length === 0) found.highlights.delete(name);
  else found.highlights.set(name, new found.Highlight(...ranges));
}

export function clearPaint(): void {
  for (const name of [MATCH_HIGHLIGHT, CURRENT_HIGHLIGHT, PASSAGE_HIGHLIGHT]) paint(name, []);
}
