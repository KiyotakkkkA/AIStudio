import type { ChatToolCallDto, Json } from "@zvs/shared";
import type { RunStep } from "../scenarios/runModel";
import type { SessionApproval } from "./sessionModel";

type JsonObject = { readonly [key: string]: Json };

function object(value: Json | null): JsonObject | null {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value : null;
}

function withoutOwner(input: Json): Json {
  const args = object(input);
  if (args === null) return input;
  return Object.fromEntries(Object.entries(args).filter(([key]) => key !== "conversationId"));
}

function resultText(output: Json | null): string | null {
  const text = object(output)?.text;
  return typeof text === "string" && text.length > 0 ? text : null;
}

function statusOf(step: RunStep, waiting: boolean): ChatToolCallDto["status"] {
  if (step.status === "running") return waiting ? "awaiting-approval" : "running";
  if (step.status === "succeeded") return "succeeded";
  if (step.status === "failed") return "failed";
  return "cancelled";
}

export function callOf(step: RunStep, approval: SessionApproval | null = null): ChatToolCallDto {
  const waiting = step.status === "running" && approval?.subject === step.type;
  return {
    id: step.id,
    key: step.type,
    name: step.type,
    round: 0,
    arguments: withoutOwner(step.input),
    status: statusOf(step, waiting),
    result: resultText(step.output),
    truncated: false,
    error: step.error,
    approvalId: waiting && approval !== null ? approval.id : null,
    stepId: step.id,
    durationMs: step.finishedAt === null ? 0 : Math.max(0, step.finishedAt - step.startedAt),
    meta: step.output,
  };
}
