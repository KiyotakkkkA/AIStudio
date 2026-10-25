import {
  AGENTIC_DEFAULT_MINUTES,
  AGENTIC_DEFAULT_STEPS,
  AGENTIC_DEFAULT_TOKENS,
  AGENTIC_MINUTES_MAX,
  AGENTIC_STEPS_MAX,
  AGENTIC_TOKENS_MAX,
  AgenticSessionDto,
  FileChangesDto,
  ProcessPlanDto,
  type AgenticBudgetDto,
  type AgenticPlanEditDto,
  type AgenticPlanStepStatus,
  type AgenticUsageDto,
  type FileDiffDto,
  type HostEvent,
  type Json,
} from "@zvs/shared";
import { stepFromEvent, type RunStep } from "../scenarios/runModel";

const AGENTIC_DOMAIN = "agentic";
const CALL_PREFIX = "session.";

export type StreamEntry =
  | { readonly kind: "text"; readonly id: string; readonly text: string }
  | { readonly kind: "tool"; readonly id: string; readonly step: RunStep };

export interface SessionApproval {
  readonly id: string;
  readonly runId: string;
  readonly subject: string;
  readonly action: string | null;
  readonly changes: readonly FileDiffDto[];
  readonly process: ProcessPlanDto | null;
}

export interface StreamState {
  readonly entries: readonly StreamEntry[];
  readonly approval: SessionApproval | null;
  readonly seqs: Readonly<Record<string, number>>;
  readonly open: string | null;
}

export interface StreamUpdate {
  readonly state: StreamState;
  readonly session: AgenticSessionDto | null;
  readonly ended: boolean;
}

export const EMPTY_STREAM: StreamState = { entries: [], approval: null, seqs: {}, open: null };

type Payload = Readonly<Record<string, Json>>;

function text(value: Json | undefined): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

function approvalFrom(request: Payload): SessionApproval | null {
  const id = text(request.id);
  const runId = text(request.runId);
  const subject = text(request.subject);
  if (id === null || runId === null || subject === null) return null;
  const changes = FileChangesDto.safeParse({ changes: request.changes });
  const process = ProcessPlanDto.safeParse(request.process);
  return {
    id,
    runId,
    subject,
    action: text(request.action),
    changes: changes.success ? changes.data.changes : [],
    process: process.success ? process.data : null,
  };
}

function sessionFromStep(step: Payload): AgenticSessionDto | null {
  if (step.domain !== AGENTIC_DOMAIN) return null;
  const parsed = AgenticSessionDto.safeParse(step.session);
  return parsed.success ? parsed.data : null;
}

function upsertTool(entries: readonly StreamEntry[], step: RunStep): StreamEntry[] {
  const index = entries.findIndex((entry) => entry.kind === "tool" && entry.id === step.id);
  const entry: StreamEntry = { kind: "tool", id: step.id, step };
  if (index === -1) return [...entries, entry];
  return entries.map((current, position) => (position === index ? entry : current));
}

function appendText(state: StreamState, id: string, delta: string): StreamState {
  const last = state.entries.at(-1);
  if (state.open !== null && last?.kind === "text" && last.id === state.open) {
    const entries = [...state.entries.slice(0, -1), { ...last, text: last.text + delta }];
    return { ...state, entries };
  }
  return { ...state, entries: [...state.entries, { kind: "text", id, text: delta }], open: id };
}

export function applyEvent(state: StreamState, event: HostEvent): StreamUpdate {
  const last = state.seqs[event.streamId];
  if (last !== undefined && event.seq <= last) return { state, session: null, ended: false };
  const next: StreamState = { ...state, seqs: { ...state.seqs, [event.streamId]: event.seq } };
  switch (event.type) {
    case "token":
      if (event.kind === "reasoning" || event.delta.length === 0) {
        return { state: next, session: null, ended: false };
      }
      return {
        state: appendText(next, `${event.streamId}:${String(event.seq)}`, event.delta),
        session: null,
        ended: false,
      };
    case "step": {
      const session = sessionFromStep(event.step);
      if (session !== null) return { state: { ...next, open: null }, session, ended: false };
      const step = stepFromEvent(event.step);
      if (step === null || !step.nodeId.startsWith(CALL_PREFIX)) {
        return { state: next, session: null, ended: false };
      }
      const approval = step.status === "running" ? next.approval : null;
      return {
        state: { ...next, entries: upsertTool(next.entries, step), approval, open: null },
        session: null,
        ended: false,
      };
    }
    case "approval":
      return {
        state: { ...next, approval: approvalFrom(event.request) ?? next.approval },
        session: null,
        ended: false,
      };
    case "end":
      return { state: { ...next, approval: null, open: null }, session: null, ended: true };
    default:
      return { state: next, session: null, ended: false };
  }
}

export function replay(
  state: StreamState,
  events: readonly HostEvent[],
): { readonly state: StreamState; readonly session: AgenticSessionDto | null } {
  let current = state;
  let session: AgenticSessionDto | null = null;
  for (const event of events) {
    const update = applyEvent(current, event);
    current = update.state;
    if (update.session !== null) session = newerSession(session, update.session);
  }
  return { state: current, session };
}

export function newerSession(
  current: AgenticSessionDto | null,
  next: AgenticSessionDto,
): AgenticSessionDto {
  if (current === null || current.id !== next.id) return next;
  return next.updatedAt >= current.updatedAt ? next : current;
}

export interface PlanRow {
  readonly key: string;
  readonly id: string | null;
  readonly title: string;
  readonly status: AgenticPlanStepStatus;
  readonly summary: string | null;
  readonly calls: number;
  readonly editable: boolean;
}

type PlanSource = Pick<AgenticSessionDto, "plan" | "pendingPlan">;

export function queuedEdits(session: PlanSource): AgenticPlanEditDto[] {
  const fixed = new Set(
    session.plan.filter((step) => step.status !== "queued").map((step) => step.id),
  );
  if (session.pendingPlan !== null) {
    return session.pendingPlan.filter((edit) => edit.id === undefined || !fixed.has(edit.id));
  }
  return session.plan
    .filter((step) => step.status === "queued")
    .map((step) => ({ id: step.id, title: step.title }));
}

export function planRows(session: PlanSource, editable: boolean): PlanRow[] {
  const fixed: PlanRow[] = session.plan
    .filter((step) => step.status !== "queued")
    .map((step) => ({
      key: step.id,
      id: step.id,
      title: step.title,
      status: step.status,
      summary: step.summary,
      calls: step.stepIds.length,
      editable: false,
    }));
  const queued: PlanRow[] = queuedEdits(session).map((edit, index) => ({
    key: edit.id ?? `new-${String(index)}`,
    id: edit.id ?? null,
    title: edit.title,
    status: "queued",
    summary: null,
    calls: 0,
    editable,
  }));
  return [...fixed, ...queued];
}

export function moveEdit(
  edits: readonly AgenticPlanEditDto[],
  from: number,
  to: number,
): AgenticPlanEditDto[] {
  if (from === to || from < 0 || from >= edits.length || to < 0 || to >= edits.length) {
    return [...edits];
  }
  const next = [...edits];
  const [moved] = next.splice(from, 1);
  if (moved !== undefined) next.splice(to, 0, moved);
  return next;
}

export function renameEdit(
  edits: readonly AgenticPlanEditDto[],
  index: number,
  title: string,
): AgenticPlanEditDto[] {
  const trimmed = title.trim();
  if (trimmed.length === 0) return [...edits];
  return edits.map((edit, position) => (position === index ? { ...edit, title: trimmed } : edit));
}

export function dropEdit(
  edits: readonly AgenticPlanEditDto[],
  index: number,
): AgenticPlanEditDto[] {
  return edits.filter((_, position) => position !== index);
}

export interface PlanProgress {
  readonly done: number;
  readonly total: number;
  readonly current: number | null;
}

export function planProgress(rows: readonly PlanRow[]): PlanProgress {
  const running = rows.findIndex((row) => row.status === "running");
  return {
    done: rows.filter((row) => row.status === "done").length,
    total: rows.length,
    current: running === -1 ? null : running + 1,
  };
}

export type SessionPhase =
  "planning" | "running" | "approval" | "budget" | "stopped" | "failed" | "cancelled" | "completed";

export function sessionPhase(
  session: Pick<AgenticSessionDto, "status" | "plan">,
  approval: SessionApproval | null,
): SessionPhase {
  switch (session.status) {
    case "running":
      if (approval !== null) return "approval";
      return session.plan.length === 0 ? "planning" : "running";
    case "paused":
      return "budget";
    case "completed":
      return "completed";
    case "stopped":
      return "stopped";
    case "failed":
      return "failed";
    case "cancelled":
      return "cancelled";
  }
}

export type BudgetKey = "steps" | "tokens" | "time";

export interface BudgetLine {
  readonly key: BudgetKey;
  readonly used: number;
  readonly limit: number;
  readonly ratio: number;
}

function ratio(used: number, limit: number): number {
  return limit <= 0 ? 1 : Math.min(1, used / limit);
}

export function budgetLines(budget: AgenticBudgetDto, usage: AgenticUsageDto): BudgetLine[] {
  const timeLimit = budget.maxMinutes * 60_000;
  return [
    {
      key: "steps",
      used: usage.steps,
      limit: budget.maxSteps,
      ratio: ratio(usage.steps, budget.maxSteps),
    },
    {
      key: "tokens",
      used: usage.tokens,
      limit: budget.maxTokens,
      ratio: ratio(usage.tokens, budget.maxTokens),
    },
    {
      key: "time",
      used: usage.elapsedMs,
      limit: timeLimit,
      ratio: ratio(usage.elapsedMs, timeLimit),
    },
  ];
}

export function extendedBudget(budget: AgenticBudgetDto, usage: AgenticUsageDto): AgenticBudgetDto {
  const minutes = Math.ceil(usage.elapsedMs / 60_000);
  return {
    maxSteps: Math.min(
      AGENTIC_STEPS_MAX,
      Math.max(budget.maxSteps, usage.steps) + AGENTIC_DEFAULT_STEPS,
    ),
    maxTokens: Math.min(
      AGENTIC_TOKENS_MAX,
      Math.max(budget.maxTokens, usage.tokens) + AGENTIC_DEFAULT_TOKENS,
    ),
    maxMinutes: Math.min(
      AGENTIC_MINUTES_MAX,
      Math.max(budget.maxMinutes, minutes) + AGENTIC_DEFAULT_MINUTES,
    ),
  };
}

export function relativePath(path: string, workspace: string): string {
  const base = workspace.replace(/[\\/]+$/, "");
  const lower = path.toLowerCase();
  const prefix = base.toLowerCase();
  if (lower.startsWith(prefix) && (path[base.length] === "\\" || path[base.length] === "/")) {
    return path.slice(base.length + 1);
  }
  return path;
}

export function runningCall(entries: readonly StreamEntry[]): RunStep | null {
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const entry = entries[index];
    if (entry?.kind === "tool" && entry.step.status === "running") return entry.step;
  }
  return null;
}
