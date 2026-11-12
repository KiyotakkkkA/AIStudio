import { z } from "zod";
import { RunId } from "./primitives/branded.js";

export const EXEC_JUDGE_ID_MAX = 200;
export const TOOL_AUTHORISATION_SOURCE_MAX = 200;
export const TOOL_AUTHORISATION_REASON_MAX = 2000;
export const POLICY_RULE_ID_MAX = 200;
export const POLICY_RULE_TEXT_MAX = 2000;
export const POLICY_PACK_NAME_MAX = 255;
export const USER_RULES_MAX = 4096;

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

export const ALLOW_RULE_SCOPES = ["folder", "anywhere"] as const;
export const AllowRuleScope = z.enum(ALLOW_RULE_SCOPES);
export type AllowRuleScope = z.infer<typeof AllowRuleScope>;

export const AllowRuleInput = z.object({
  runId: RunId,
  approvalId: z.uuid(),
  scope: AllowRuleScope,
});
export type AllowRuleInput = z.infer<typeof AllowRuleInput>;

export const PolicyRuleDto = z.object({
  id: z.string().min(1).max(POLICY_RULE_ID_MAX),
  text: z.string().min(1).max(POLICY_RULE_TEXT_MAX),
});
export type PolicyRuleDto = z.infer<typeof PolicyRuleDto>;

export const POLICY_RULE_EFFECTS = ["allow", "review", "ask", "deny"] as const;
export const PolicyRuleEffect = z.enum(POLICY_RULE_EFFECTS);
export type PolicyRuleEffect = z.infer<typeof PolicyRuleEffect>;

const PackName = z
  .string()
  .min(1)
  .max(POLICY_PACK_NAME_MAX)
  .refine((name) => !/[/\\:]/.test(name) && !name.startsWith("."));

export const UserRuleDto = z.object({
  id: z.string().min(1).max(POLICY_RULE_ID_MAX),
  text: z.string().min(1).max(POLICY_RULE_TEXT_MAX),
  effect: PolicyRuleEffect,
  pack: PackName,
  createdAt: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER).nullable(),
  active: z.boolean(),
});
export type UserRuleDto = z.infer<typeof UserRuleDto>;

export const UserRulesDto = z.object({
  rules: z.array(UserRuleDto).max(USER_RULES_MAX),
  unconfirmed: z.array(PackName),
});
export type UserRulesDto = z.infer<typeof UserRulesDto>;

export const RemoveRuleInput = z.object({ id: z.string().min(1).max(POLICY_RULE_ID_MAX) });
export type RemoveRuleInput = z.infer<typeof RemoveRuleInput>;

export const ConfirmRulePackInput = z.object({ pack: PackName });
export type ConfirmRulePackInput = z.infer<typeof ConfirmRulePackInput>;
