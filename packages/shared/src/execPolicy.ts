import { z } from "zod";
import { FileChangeKind } from "./files.js";
import { FolderPath } from "./folders.js";

export const EXEC_JUDGE_ID_MAX = 200;
export const TOOL_AUTHORISATION_SOURCE_MAX = 200;
export const TOOL_AUTHORISATION_REASON_MAX = 2000;
export const ALLOW_RULE_CHANGES_MAX = 64;
export const POLICY_RULE_ID_MAX = 200;
export const POLICY_RULE_TEXT_MAX = 2000;

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

export const AllowRuleChangeDto = z.object({ path: FolderPath, kind: FileChangeKind });
export type AllowRuleChangeDto = z.infer<typeof AllowRuleChangeDto>;

export const AllowRuleInput = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("files"),
    changes: z.array(AllowRuleChangeDto).min(1).max(ALLOW_RULE_CHANGES_MAX),
    folder: FolderPath.nullable(),
  }),
  z.object({
    kind: z.literal("process"),
    argv: z
      .array(z.string().max(4096))
      .min(1)
      .max(64)
      .refine(
        (argv) => {
          const program = argv[0] ?? "";
          return program !== "" && program.length <= 32767;
        },
        { path: [0] },
      ),
    cwd: FolderPath,
    folder: FolderPath.nullable(),
  }),
]);
export type AllowRuleInput = z.infer<typeof AllowRuleInput>;

export const PolicyRuleDto = z.object({
  id: z.string().min(1).max(POLICY_RULE_ID_MAX),
  text: z.string().min(1).max(POLICY_RULE_TEXT_MAX),
});
export type PolicyRuleDto = z.infer<typeof PolicyRuleDto>;
