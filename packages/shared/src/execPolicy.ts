import { z } from "zod";

export const EXEC_JUDGE_ID_MAX = 200;
export const TOOL_AUTHORISATION_SOURCE_MAX = 200;
export const TOOL_AUTHORISATION_REASON_MAX = 2000;

export const EXEC_MODES = ["always-ask", "safety-ask", "auto-bypass", "no-restrictions"] as const;
export const ExecMode = z.enum(EXEC_MODES);
export type ExecMode = z.infer<typeof ExecMode>;

export const ExecJudgeDto = z.object({
  providerId: z.string().min(1).max(EXEC_JUDGE_ID_MAX),
  modelId: z.string().min(1).max(EXEC_JUDGE_ID_MAX),
});
export type ExecJudgeDto = z.infer<typeof ExecJudgeDto>;

export const ExecPolicyDto = z.object({
  mode: ExecMode,
  judge: ExecJudgeDto.nullable(),
});
export type ExecPolicyDto = z.infer<typeof ExecPolicyDto>;

export const TOOL_AUTHORISATION_KINDS = ["auto", "granted", "approved", "policy", "judge"] as const;
export const ToolAuthorisationKind = z.enum(TOOL_AUTHORISATION_KINDS);
export type ToolAuthorisationKind = z.infer<typeof ToolAuthorisationKind>;

export const ToolAuthorisationDto = z.object({
  kind: ToolAuthorisationKind,
  source: z.string().min(1).max(TOOL_AUTHORISATION_SOURCE_MAX).optional(),
  reason: z.string().max(TOOL_AUTHORISATION_REASON_MAX).optional(),
});
export type ToolAuthorisationDto = z.infer<typeof ToolAuthorisationDto>;
