import { z } from "zod";
import { FolderPath } from "./folders.js";
import { AgenticSessionId, ProviderId, RunId, StepId, StreamId } from "./primitives/branded.js";
import { ToolKey } from "./tools/ToolDto.js";

export const AGENTIC_GOAL_MAX = 4000;
export const AGENTIC_STEP_TITLE_MAX = 200;
export const AGENTIC_SUMMARY_MAX = 2000;
export const AGENTIC_INSTRUCTION_MAX = 4000;
export const AGENTIC_PLAN_MAX = 50;
export const AGENTIC_TOOLS_MAX = 256;
export const AGENTIC_STEPS_MAX = 1000;
export const AGENTIC_TOKENS_MIN = 1000;
export const AGENTIC_TOKENS_MAX = 100_000_000;
export const AGENTIC_MINUTES_MAX = 1440;
export const AGENTIC_DEFAULT_STEPS = 25;
export const AGENTIC_DEFAULT_TOKENS = 200_000;
export const AGENTIC_DEFAULT_MINUTES = 15;

const trimmed = (max: number) =>
  z
    .string()
    .refine((value) => value.trim().length > 0)
    .refine((value) => value.trim().length <= max);

const Count = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER);
const PlanStepId = z.string().min(1).max(16);

export const AgenticPermissionMode = z.enum(["read-only", "ask", "auto"]);
export type AgenticPermissionMode = z.infer<typeof AgenticPermissionMode>;

export const AgenticSessionStatus = z.enum([
  "running",
  "paused",
  "completed",
  "stopped",
  "failed",
  "cancelled",
]);
export type AgenticSessionStatus = z.infer<typeof AgenticSessionStatus>;

export const AgenticStopReason = z.enum([
  "goal",
  "steps",
  "tokens",
  "time",
  "loop",
  "cancelled",
  "error",
]);
export type AgenticStopReason = z.infer<typeof AgenticStopReason>;

export const AgenticPlanStepStatus = z.enum(["queued", "running", "done", "failed"]);
export type AgenticPlanStepStatus = z.infer<typeof AgenticPlanStepStatus>;

export const AgenticBudgetDto = z.object({
  maxSteps: z.number().int().min(1).max(AGENTIC_STEPS_MAX),
  maxTokens: z.number().int().min(AGENTIC_TOKENS_MIN).max(AGENTIC_TOKENS_MAX),
  maxMinutes: z.number().int().min(1).max(AGENTIC_MINUTES_MAX),
});
export type AgenticBudgetDto = z.infer<typeof AgenticBudgetDto>;

export const AgenticUsageDto = z.object({
  steps: Count,
  tokens: Count,
  elapsedMs: Count,
  toolCalls: Count,
});
export type AgenticUsageDto = z.infer<typeof AgenticUsageDto>;

export const AgenticPlanStepDto = z.object({
  id: PlanStepId,
  title: trimmed(AGENTIC_STEP_TITLE_MAX),
  status: AgenticPlanStepStatus,
  stepIds: z.array(StepId),
  summary: z.string().max(AGENTIC_SUMMARY_MAX).nullable(),
});
export type AgenticPlanStepDto = z.infer<typeof AgenticPlanStepDto>;

export const AgenticPlanEditDto = z.object({
  id: PlanStepId.optional(),
  title: trimmed(AGENTIC_STEP_TITLE_MAX),
});
export type AgenticPlanEditDto = z.infer<typeof AgenticPlanEditDto>;

export const AgenticChangedFileDto = z.object({
  path: FolderPath,
  added: Count,
  removed: Count,
});
export type AgenticChangedFileDto = z.infer<typeof AgenticChangedFileDto>;

export const AgenticSessionDto = z
  .object({
    id: AgenticSessionId,
    goal: trimmed(AGENTIC_GOAL_MAX),
    status: AgenticSessionStatus,
    stopReason: AgenticStopReason.nullable(),
    error: z.string().nullable(),
    mode: AgenticPermissionMode,
    pendingMode: AgenticPermissionMode.nullable(),
    workspace: FolderPath,
    providerId: ProviderId,
    modelId: z.string().min(1),
    tools: z.array(z.string()),
    budget: AgenticBudgetDto,
    usage: AgenticUsageDto,
    plan: z.array(AgenticPlanStepDto),
    pendingPlan: z.array(AgenticPlanEditDto).nullable(),
    pendingInstructions: z.array(z.string()),
    changedFiles: z.array(AgenticChangedFileDto),
    runId: RunId.nullable(),
    streamId: StreamId.nullable(),
    summary: z.string().nullable(),
    createdAt: z.number().int(),
    updatedAt: z.number().int(),
  })
  .refine((session) => (session.status === "running") === (session.stopReason === null), {
    path: ["stopReason"],
  })
  .refine((session) => (session.runId === null) === (session.streamId === null), {
    path: ["streamId"],
  });
export type AgenticSessionDto = z.infer<typeof AgenticSessionDto>;

export const CreateAgenticSessionInput = z.object({
  goal: trimmed(AGENTIC_GOAL_MAX),
  workspace: FolderPath,
  providerId: ProviderId,
  modelId: z.string().min(1),
  mode: AgenticPermissionMode,
  budget: AgenticBudgetDto.optional(),
  tools: z.array(ToolKey).max(AGENTIC_TOOLS_MAX).optional(),
});
export type CreateAgenticSessionInput = z.infer<typeof CreateAgenticSessionInput>;

export const AgenticSessionRef = z.object({ id: AgenticSessionId });
export type AgenticSessionRef = z.infer<typeof AgenticSessionRef>;

export const UpdateAgenticPlanInput = z.object({
  id: AgenticSessionId,
  steps: z.array(AgenticPlanEditDto).max(AGENTIC_PLAN_MAX),
});
export type UpdateAgenticPlanInput = z.infer<typeof UpdateAgenticPlanInput>;

export const SetAgenticModeInput = z.object({
  id: AgenticSessionId,
  mode: AgenticPermissionMode,
});
export type SetAgenticModeInput = z.infer<typeof SetAgenticModeInput>;

export const AgenticInstructInput = z.object({
  id: AgenticSessionId,
  text: trimmed(AGENTIC_INSTRUCTION_MAX),
});
export type AgenticInstructInput = z.infer<typeof AgenticInstructInput>;

export const ExtendAgenticSessionInput = z.object({
  id: AgenticSessionId,
  budget: AgenticBudgetDto,
});
export type ExtendAgenticSessionInput = z.infer<typeof ExtendAgenticSessionInput>;
