import type { Json, LogLineDto, ScenarioGraphDto, StepDto, StepEventDto } from "@zvs/shared";
import { edgeLinkId } from "./graphModel";

export type StepStatus = StepDto["status"];
export type RunPhase = "running" | "succeeded" | "failed" | "cancelled";
export type NodeRunStatus = "running" | "succeeded" | "failed";
export type RunTab = "input" | "output" | "logs";

export interface RunStep {
  readonly id: string;
  readonly nodeId: string;
  readonly type: string;
  readonly status: StepStatus;
  readonly input: Json;
  readonly output: Json | null;
  readonly error: string | null;
  readonly startedAt: number;
  readonly finishedAt: number | null;
  readonly attempt: number;
}

export interface RunLog {
  readonly nodeId: string | null;
  readonly level: string;
  readonly message: string;
  readonly line: LogLineDto;
}

export interface RunView {
  readonly runId: string;
  readonly streamId: string;
  readonly phase: RunPhase;
  readonly steps: readonly RunStep[];
  readonly logs: readonly RunLog[];
  readonly startedAt: number;
  readonly finishedAt: number | null;
  readonly error: string | null;
  readonly code: string | null;
  readonly retryOf: string | null;
}

export interface NodeRunState {
  readonly status: NodeRunStatus;
  readonly error: string | null;
}

export interface EdgeRunState {
  readonly traversed: boolean;
  readonly items: number | null;
}

export interface StepRow {
  readonly step: RunStep;
  readonly depth: number;
  readonly duration: number | null;
}

export const STEP_PATH_SEPARATOR = ".";

const STEP_STATUSES: readonly StepStatus[] = [
  "running",
  "succeeded",
  "failed",
  "cancelled",
  "abandoned",
];

function text(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function number(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

export function stepFromEvent(event: StepEventDto): RunStep | null {
  const id = text(event.id);
  const nodeId = text(event.nodeId);
  const status = STEP_STATUSES.find((entry) => entry === event.status);
  const startedAt = number(event.startedAt);
  if (id === null || nodeId === null || status === undefined || startedAt === null) return null;
  const output = event.output as Json | undefined;
  return {
    id,
    nodeId,
    type: text(event.type) ?? "",
    status,
    input: (event.input as Json | undefined) ?? null,
    output: output === undefined ? null : output,
    error: text(event.error),
    startedAt,
    finishedAt: number(event.finishedAt),
    attempt: number(event.attempt) ?? 1,
  };
}

export function stepFromDto(step: StepDto): RunStep {
  return {
    id: step.id,
    nodeId: step.nodeId,
    type: step.type,
    status: step.status,
    input: step.input,
    output: step.output ?? null,
    error: step.error ?? null,
    startedAt: step.startedAt,
    finishedAt: step.finishedAt ?? null,
    attempt: step.attempt,
  };
}

export function logFromLine(line: LogLineDto): RunLog | null {
  const message = text(line.message);
  if (message === null) return null;
  return {
    nodeId: text(line.nodeId),
    level: text(line.level) ?? "info",
    message,
    line,
  };
}

export function phaseOf(status: string): RunPhase {
  if (status === "succeeded") return "succeeded";
  if (status === "failed" || status === "interrupted") return "failed";
  if (status === "cancelled") return "cancelled";
  return "running";
}

export function startView(runId: string, streamId: string, startedAt: number): RunView {
  return {
    runId,
    streamId,
    phase: "running",
    steps: [],
    logs: [],
    startedAt,
    finishedAt: null,
    error: null,
    code: null,
    retryOf: null,
  };
}

export function withStep(view: RunView, step: RunStep): RunView {
  const index = view.steps.findIndex((entry) => entry.id === step.id);
  const steps =
    index === -1
      ? [...view.steps, step]
      : view.steps.map((entry, position) => (position === index ? step : entry));
  return { ...view, steps };
}

export function withLog(view: RunView, log: RunLog): RunView {
  return { ...view, logs: [...view.logs, log] };
}

export function finishView(
  view: RunView,
  phase: Exclude<RunPhase, "running">,
  finishedAt: number,
  error: string | null,
  code: string | null = null,
): RunView {
  const steps = view.steps.map((step) =>
    step.status === "running" ? { ...step, status: "cancelled" as const } : step,
  );
  return { ...view, phase, finishedAt, error, code, steps };
}

export function isNested(nodeId: string): boolean {
  return nodeId.includes(STEP_PATH_SEPARATOR);
}

export function latestSteps(view: RunView): ReadonlyMap<string, RunStep> {
  const latest = new Map<string, RunStep>();
  for (const step of view.steps) {
    if (isNested(step.nodeId)) continue;
    const previous = latest.get(step.nodeId);
    if (
      previous === undefined ||
      step.attempt > previous.attempt ||
      (step.attempt === previous.attempt && step.startedAt >= previous.startedAt)
    ) {
      latest.set(step.nodeId, step);
    }
  }
  return latest;
}

export function nodeStates(view: RunView | null): ReadonlyMap<string, NodeRunState> {
  const states = new Map<string, NodeRunState>();
  if (view === null) return states;
  for (const [nodeId, step] of latestSteps(view)) {
    if (step.status === "running") {
      states.set(nodeId, { status: view.phase === "running" ? "running" : "failed", error: null });
    } else if (step.status === "succeeded") {
      states.set(nodeId, { status: "succeeded", error: null });
    } else {
      states.set(nodeId, { status: "failed", error: step.error });
    }
  }
  return states;
}

const ITEM_KEYS = ["items", "hits", "results", "documents", "messages"];

export function itemCount(output: Json | null): number {
  if (output === null) return 0;
  if (Array.isArray(output)) return output.length;
  if (typeof output === "object") {
    for (const key of ITEM_KEYS) {
      const value = output[key];
      if (Array.isArray(value)) return value.length;
    }
  }
  return 1;
}

export function edgeStates(
  graph: ScenarioGraphDto,
  view: RunView | null,
): ReadonlyMap<string, EdgeRunState> {
  const states = new Map<string, EdgeRunState>();
  if (view === null) return states;
  const latest = latestSteps(view);
  for (const edge of graph.edges) {
    const source = latest.get(edge.from);
    if (source?.status !== "succeeded" || !latest.has(edge.to)) continue;
    states.set(edgeLinkId(edge), { traversed: true, items: itemCount(source.output) });
  }
  return states;
}

export function stepRows(view: RunView | null): StepRow[] {
  if (view === null) return [];
  const ordered = [...view.steps].sort(
    (left, right) => left.startedAt - right.startedAt || left.attempt - right.attempt,
  );
  return ordered.map((step) => ({
    step,
    depth: step.nodeId.split(STEP_PATH_SEPARATOR).length - 1,
    duration: step.finishedAt === null ? null : Math.max(0, step.finishedAt - step.startedAt),
  }));
}

export function failedStep(view: RunView | null): RunStep | null {
  if (view === null) return null;
  const failed = [...latestSteps(view).values()].filter((step) => step.status === "failed");
  return failed.sort((left, right) => left.startedAt - right.startedAt)[0] ?? null;
}

export function upstreamOutput(
  graph: ScenarioGraphDto,
  view: RunView | null,
  nodeId: string,
): Json | undefined {
  if (view === null) return undefined;
  const latest = latestSteps(view);
  const sources = graph.edges
    .filter((edge) => edge.to === nodeId)
    .map((edge) => latest.get(edge.from))
    .filter((step): step is RunStep => step?.status === "succeeded");
  if (sources.length === 0) return undefined;
  if (sources.length === 1) return sources[0]?.output ?? null;
  return Object.fromEntries(sources.map((step) => [step.nodeId, step.output]));
}

export function runDuration(view: RunView | null, now: number): number | null {
  if (view === null) return null;
  return Math.max(0, (view.finishedAt ?? now) - view.startedAt);
}

export function stepLogs(view: RunView | null, nodeId: string): RunLog[] {
  if (view === null) return [];
  return view.logs.filter(
    (log) =>
      log.nodeId !== null &&
      (log.nodeId === nodeId || log.nodeId.startsWith(`${nodeId}${STEP_PATH_SEPARATOR}`)),
  );
}
