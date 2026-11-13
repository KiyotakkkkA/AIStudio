export type HistoryStep = "PUSH" | "POP" | "REPLACE";

export interface HistoryPosition {
  readonly index: number;
  readonly last: number;
}

export function historyIndex(state: unknown): number {
  if (typeof state !== "object" || state === null) return 0;
  const index = (state as { idx?: unknown }).idx;
  return typeof index === "number" && Number.isInteger(index) && index >= 0 ? index : 0;
}

export function startPosition(index: number): HistoryPosition {
  return { index, last: index };
}

export function stepPosition(
  position: HistoryPosition,
  step: HistoryStep,
  index: number,
): HistoryPosition {
  if (step === "PUSH") return { index, last: index };
  return { index, last: Math.max(position.last, index) };
}

export function canGoBack(position: HistoryPosition): boolean {
  return position.index > 0;
}

export function canGoForward(position: HistoryPosition): boolean {
  return position.index < position.last;
}
