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
