import type { RunDto, RunSummaryDto } from "@zvs/shared";

export type TaskGroup = "running" | "queued" | "attention" | "finished";
export type TaskStatusFilter = "active" | "running" | "queued" | "blocked" | "failed";

const GROUPS: Record<RunDto["status"], TaskGroup> = {
  running: "running",
  queued: "queued",
  blocked: "attention",
  failed: "attention",
  interrupted: "attention",
  succeeded: "finished",
  cancelled: "finished",
};

const FILTERS: Record<TaskStatusFilter, readonly RunDto["status"][]> = {
  active: ["running", "queued", "blocked", "failed", "interrupted"],
  running: ["running"],
  queued: ["queued"],
  blocked: ["blocked"],
  failed: ["failed", "interrupted"],
};

export function groupOf(run: Pick<RunSummaryDto, "status">): TaskGroup {
  return GROUPS[run.status];
}

export function matchesStatusFilter(
  run: Pick<RunSummaryDto, "status">,
  filter: TaskStatusFilter,
): boolean {
  return FILTERS[filter].includes(run.status);
}

export function statusesOf(filter: TaskStatusFilter): readonly RunDto["status"][] {
  return FILTERS[filter];
}

export interface NestedRun<T> {
  readonly run: T;
  readonly nested: boolean;
}

type Nestable = Pick<RunSummaryDto, "id" | "parentRunId">;

export function nestRuns<T extends Nestable>(
  runs: readonly T[],
): { roots: T[]; children: Map<string, T[]> } {
  const known = new Set(runs.map((run) => run.id));
  const roots: T[] = [];
  const children = new Map<string, T[]>();
  for (const run of runs) {
    const parent = run.parentRunId;
    if (parent !== undefined && known.has(parent)) {
      const siblings = children.get(parent) ?? [];
      siblings.push(run);
      children.set(parent, siblings);
    } else {
      roots.push(run);
    }
  }
  for (const siblings of children.values())
    siblings.sort((left, right) => (left.id < right.id ? -1 : left.id > right.id ? 1 : 0));
  return { roots, children };
}

export function flattenRuns<T extends Nestable>(runs: readonly T[]): NestedRun<T>[] {
  const { roots, children } = nestRuns(runs);
  return roots.flatMap((run) => [
    { run, nested: false },
    ...(children.get(run.id) ?? []).map((child) => ({ run: child, nested: true })),
  ]);
}

export function groupRuns<T extends Pick<RunSummaryDto, "status">>(
  runs: readonly T[],
): Record<TaskGroup, T[]> {
  const grouped: Record<TaskGroup, T[]> = {
    running: [],
    queued: [],
    attention: [],
    finished: [],
  };
  for (const run of runs) grouped[groupOf(run)].push(run);
  return grouped;
}
