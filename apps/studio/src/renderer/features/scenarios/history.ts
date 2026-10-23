import type { ScenarioGraphDto } from "@zvs/shared";

export const HISTORY_LIMIT = 100;

export interface GraphHistory {
  readonly past: readonly ScenarioGraphDto[];
  readonly present: ScenarioGraphDto;
  readonly future: readonly ScenarioGraphDto[];
}

export function startHistory(graph: ScenarioGraphDto): GraphHistory {
  return { past: [], present: graph, future: [] };
}

export function commit(
  history: GraphHistory,
  graph: ScenarioGraphDto,
  limit: number = HISTORY_LIMIT,
): GraphHistory {
  if (graph === history.present) return history;
  const past = [...history.past, history.present];
  return { past: past.slice(Math.max(0, past.length - limit)), present: graph, future: [] };
}

export function undo(history: GraphHistory): GraphHistory {
  const previous = history.past.at(-1);
  if (previous === undefined) return history;
  return {
    past: history.past.slice(0, -1),
    present: previous,
    future: [history.present, ...history.future],
  };
}

export function redo(history: GraphHistory): GraphHistory {
  const [next, ...rest] = history.future;
  if (next === undefined) return history;
  return { past: [...history.past, history.present], present: next, future: rest };
}

export function canUndo(history: GraphHistory): boolean {
  return history.past.length > 0;
}

export function canRedo(history: GraphHistory): boolean {
  return history.future.length > 0;
}
