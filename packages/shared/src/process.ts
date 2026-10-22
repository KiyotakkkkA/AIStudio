import { z } from "zod";
import { ConversationId, RunId } from "./primitives/branded.js";

export const PROCESS_APPROVAL_KIND = "process";

export const CodeLanguage = z.enum(["python", "javascript", "powershell"]);
export type CodeLanguage = z.infer<typeof CodeLanguage>;

export const ProcessPlanDto = z
  .object({
    program: z.string().min(1).max(32767),
    language: CodeLanguage.optional(),
    argv: z.array(z.string().max(4096)).max(64),
    cwd: z.string().min(1).max(32767),
    source: z.string().max(262144).optional(),
    packages: z.array(z.string().min(1).max(214)).max(32),
  })
  .refine((plan) => (plan.source === undefined) === (plan.language === undefined), {
    path: ["source"],
  })
  .refine((plan) => plan.packages.length === 0 || plan.language === "javascript", {
    path: ["packages"],
  });
export type ProcessPlanDto = z.infer<typeof ProcessPlanDto>;

export const ProcessResultDto = z.object({
  process: ProcessPlanDto,
  exitCode: z.number().int().nullable(),
  durationMs: z.number().int().nonnegative(),
  timedOut: z.boolean(),
  truncated: z.boolean(),
});
export type ProcessResultDto = z.infer<typeof ProcessResultDto>;

export const ProcessApprovalDto = z.object({
  id: z.uuid(),
  runId: RunId,
  kind: z.literal(PROCESS_APPROVAL_KIND),
  subject: z.string().min(1),
  scope: z.string(),
  expiresAt: z.number().int(),
  conversationId: ConversationId.optional(),
  tool: z.string().min(1),
  action: z.string().max(2000),
  process: ProcessPlanDto,
});
export type ProcessApprovalDto = z.infer<typeof ProcessApprovalDto>;
