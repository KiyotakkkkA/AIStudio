import { makeAutoObservable } from "mobx";
import { ChatToolCallDto, SUBAGENT_DOMAIN } from "@zvs/shared";

export const AGENT_DELEGATE = "agent.delegate";

export interface SubagentLabel {
  readonly runId: string;
  readonly label: string;
}

export interface SubagentCompactStep {
  readonly key: string;
  readonly name: string;
  readonly status: ChatToolCallDto["status"];
  readonly error?: string;
}

export type SubagentPresetKey = "explore" | "general";
export type SubagentStatus = "queued" | "running" | "ok" | "partial" | "cancelled" | "failed";

export interface SubagentMeta {
  readonly runId?: string;
  readonly preset: SubagentPresetKey;
  readonly label: string;
  readonly task: string;
  readonly model: string;
  readonly modelSource: "preferred" | "dialog";
  readonly status: SubagentStatus;
  readonly partial: boolean;
  readonly toolCalls: number;
  readonly tokens: number;
  readonly report?: string;
  readonly durationMs?: number;
  readonly calls?: readonly SubagentCompactStep[];
}

const PRESETS: readonly string[] = ["explore", "general"];
const STATUSES: readonly string[] = ["queued", "running", "ok", "partial", "cancelled", "failed"];

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function text(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function count(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : 0;
}

function compactSteps(value: unknown): SubagentCompactStep[] | undefined {
  if (!Array.isArray(value)) return undefined;
  return value.flatMap((item) => {
    const entry = record(item);
    const key = text(entry?.key);
    const name = text(entry?.name);
    const status = ChatToolCallDto.shape.status.safeParse(entry?.status);
    if (key === undefined || name === undefined || !status.success) return [];
    const error = text(entry?.error);
    return [{ key, name, status: status.data, ...(error === undefined ? {} : { error }) }];
  });
}

export function subagentMeta(call: ChatToolCallDto): SubagentMeta | null {
  if (call.key !== AGENT_DELEGATE) return null;
  const meta = record(call.meta);
  const preset = text(meta?.preset);
  const status = text(meta?.status);
  if (meta === null || preset === undefined || !PRESETS.includes(preset)) return null;
  if (status === undefined || !STATUSES.includes(status)) return null;
  const runId = text(meta.runId);
  const report = text(meta.report);
  const calls = compactSteps(meta.calls);
  return {
    preset: preset as SubagentPresetKey,
    status: status as SubagentStatus,
    label: text(meta.label) ?? "",
    task: text(meta.task) ?? "",
    model: text(meta.model) ?? "",
    modelSource: meta.modelSource === "preferred" ? "preferred" : "dialog",
    partial: meta.partial === true,
    toolCalls: count(meta.toolCalls),
    tokens: count(meta.tokens),
    ...(runId === undefined ? {} : { runId }),
    ...(report === undefined ? {} : { report }),
    ...(typeof meta.durationMs === "number" ? { durationMs: count(meta.durationMs) } : {}),
    ...(calls === undefined ? {} : { calls }),
  };
}

export function subagentLabel(request: Record<string, unknown>): SubagentLabel | null {
  const entry = record(request.subagent);
  const runId = text(entry?.runId);
  const label = text(entry?.label);
  if (runId === undefined || label === undefined || runId !== request.runId) return null;
  return { runId, label };
}

export function delegateRuns(calls: readonly ChatToolCallDto[]): ChatToolCallDto[][] {
  const groups: ChatToolCallDto[][] = [];
  for (const call of calls) {
    const last = groups.at(-1);
    const delegate = call.key === AGENT_DELEGATE;
    if (delegate && last !== undefined && last[0]?.key === AGENT_DELEGATE) last.push(call);
    else groups.push([call]);
  }
  return groups;
}

export default class SubagentSteps {
  byRun: Record<string, ChatToolCallDto[]> = {};

  constructor() {
    makeAutoObservable(this, {}, { autoBind: true });
  }

  apply(step: unknown): boolean {
    const entry = record(step);
    const runId = text(entry?.runId);
    const index = entry?.index;
    const call = ChatToolCallDto.safeParse(entry?.call);
    if (
      entry?.domain !== SUBAGENT_DOMAIN ||
      runId === undefined ||
      typeof index !== "number" ||
      !Number.isInteger(index) ||
      index < 0 ||
      !call.success
    )
      return false;
    const calls = [...(this.byRun[runId] ?? [])];
    calls[Math.min(index, calls.length)] = call.data;
    this.byRun = { ...this.byRun, [runId]: calls };
    return true;
  }

  callsFor(runId: string | undefined): ChatToolCallDto[] {
    return runId === undefined ? [] : (this.byRun[runId] ?? []);
  }

  get approvalIds(): Set<string> {
    return new Set(
      Object.values(this.byRun).flatMap((calls) =>
        calls.flatMap((call) => (call.approvalId === null ? [] : [call.approvalId])),
      ),
    );
  }

  clear(): void {
    this.byRun = {};
  }
}
