import type { ChatToolCallDto, FileDiffDto, Json, ProcessResultDto } from "@zvs/shared";
import { fileChanges, FILE_TOOL_APPLY } from "../chat/fileChangesCopy";
import { PROCESS_TOOL_CODE, PROCESS_TOOL_SHELL, processResult } from "../chat/processCopy";
import { toolCallSummary } from "../chat/toolCallSummary";
import type { StatusTone } from "../../ui/atoms/statusTone";
import type { RunStep } from "../scenarios/runModel";

const OUTPUT_PREVIEW_LINES = 12;

type JsonObject = { readonly [key: string]: Json };

function object(value: Json | null): JsonObject | null {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value : null;
}

function field(value: Json | null, name: string): Json | undefined {
  return object(value)?.[name];
}

function stringField(value: Json | null, name: string): string {
  const found = field(value, name);
  return typeof found === "string" ? found : "";
}

function withoutOwner(input: Json): Json {
  const args = object(input);
  if (args === null) return input;
  return Object.fromEntries(Object.entries(args).filter(([key]) => key !== "conversationId"));
}

function isProcessTool(key: string): boolean {
  return key === PROCESS_TOOL_CODE || key === PROCESS_TOOL_SHELL;
}

function callOf(step: RunStep): ChatToolCallDto {
  const status: ChatToolCallDto["status"] =
    step.status === "running"
      ? "running"
      : step.status === "succeeded"
        ? "succeeded"
        : step.status === "failed"
          ? "failed"
          : "cancelled";
  return {
    id: step.id,
    key: step.type,
    name: step.type,
    round: 0,
    arguments: withoutOwner(step.input),
    status,
    result: stringField(step.output, "text"),
    truncated: false,
    error: step.error,
    approvalId: null,
    stepId: step.id,
    durationMs: step.finishedAt === null ? 0 : Math.max(0, step.finishedAt - step.startedAt),
    meta: step.output,
  };
}

export function cardTarget(step: RunStep): string {
  return toolCallSummary(callOf(step));
}

export type CardBody =
  | { readonly kind: "changes"; readonly changes: readonly FileDiffDto[] }
  | {
      readonly kind: "process";
      readonly result: ProcessResultDto;
      readonly stdout: string;
      readonly stderr: string;
    }
  | { readonly kind: "text"; readonly text: string }
  | { readonly kind: "error"; readonly text: string };

export function cardBody(step: RunStep): CardBody | null {
  if (step.status === "failed" || step.status === "cancelled" || step.status === "abandoned") {
    return step.error === null ? null : { kind: "error", text: step.error };
  }
  if (step.output === null) return null;
  if (step.type === FILE_TOOL_APPLY) {
    const changes = fileChanges(step.output);
    if (changes.length > 0) return { kind: "changes", changes };
  }
  if (isProcessTool(step.type)) {
    const result = processResult(step.output);
    if (result !== null) {
      return {
        kind: "process",
        result,
        stdout: stringField(step.output, "stdout"),
        stderr: stringField(step.output, "stderr"),
      };
    }
  }
  const output = stringField(step.output, "text");
  return output.trim().length === 0 ? null : { kind: "text", text: output };
}

export interface CardResult {
  readonly tone: StatusTone;
  readonly label: string;
}

const CARD_RUNNING = "Выполняется";
const CARD_FAILED = "Ошибка";
const CARD_STOPPED = "Остановлен";
const CARD_DONE = "Готово";
const CARD_TIMED_OUT = "Время истекло";

export function exitCodeLabel(code: number): string {
  return `код ${String(code)}`;
}

function exitChip(result: ProcessResultDto): CardResult {
  if (result.timedOut) return { tone: "err", label: CARD_TIMED_OUT };
  if (result.exitCode === null) return { tone: "warn", label: "код —" };
  return { tone: result.exitCode === 0 ? "ok" : "err", label: exitCodeLabel(result.exitCode) };
}

export function cardResult(step: RunStep): CardResult | null {
  if (step.status === "running") return { tone: "accent", label: CARD_RUNNING };
  if (step.status === "failed") return { tone: "err", label: CARD_FAILED };
  if (step.status !== "succeeded") return { tone: "idle", label: CARD_STOPPED };
  if (step.type === FILE_TOOL_APPLY && fileChanges(step.output ?? null).length > 0) return null;
  if (isProcessTool(step.type)) {
    const result = processResult(step.output ?? null);
    if (result !== null) return exitChip(result);
  }
  if (field(step.output, "isError") === true) return { tone: "err", label: CARD_FAILED };
  return { tone: "ok", label: CARD_DONE };
}

export function changeTotals(changes: readonly FileDiffDto[]): {
  readonly added: number;
  readonly removed: number;
} {
  return {
    added: changes.reduce((sum, change) => sum + change.added, 0),
    removed: changes.reduce((sum, change) => sum + change.removed, 0),
  };
}

export function opensByDefault(step: RunStep): boolean {
  if (step.status === "failed") return true;
  return step.type === FILE_TOOL_APPLY || isProcessTool(step.type);
}

export interface OutputPreview {
  readonly shown: string;
  readonly hidden: number;
}

export function outputPreview(output: string, limit: number = OUTPUT_PREVIEW_LINES): OutputPreview {
  const lines = output.replace(/\n+$/, "").split("\n");
  if (lines.length <= limit) return { shown: lines.join("\n"), hidden: 0 };
  return { shown: lines.slice(0, limit).join("\n"), hidden: lines.length - limit };
}
