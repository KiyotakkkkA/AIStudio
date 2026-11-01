import { z } from "zod";
import { RunId } from "./primitives/branded.js";

export const SUBAGENT_DOMAIN = "subagent";
export const SUBAGENT_MODELS_MAX = 5;
export const SUBAGENT_CONCURRENCY_MIN = 1;
export const SUBAGENT_CONCURRENCY_MAX = 4;
export const SUBAGENT_CONCURRENCY_DEFAULT = 2;
export const SUBAGENT_MODEL_ID_MAX = 200;

export const SubagentModelDto = z.object({
  providerId: z.string().min(1).max(SUBAGENT_MODEL_ID_MAX),
  modelId: z.string().min(1).max(SUBAGENT_MODEL_ID_MAX),
});
export type SubagentModelDto = z.infer<typeof SubagentModelDto>;

export const SubagentPreferencesDto = z.object({
  enabled: z.boolean(),
  models: z
    .array(SubagentModelDto)
    .max(SUBAGENT_MODELS_MAX)
    .superRefine((models, context) => {
      models.forEach((model, index) => {
        const duplicate = models
          .slice(0, index)
          .some(
            (other) => other.providerId === model.providerId && other.modelId === model.modelId,
          );
        if (duplicate) context.addIssue({ code: "custom", path: [index], message: "unique" });
      });
    }),
  concurrency: z.number().int().min(SUBAGENT_CONCURRENCY_MIN).max(SUBAGENT_CONCURRENCY_MAX),
});
export type SubagentPreferencesDto = z.infer<typeof SubagentPreferencesDto>;

export const CliSubagentCancelInput = z.object({ runId: RunId });
export type CliSubagentCancelInput = z.infer<typeof CliSubagentCancelInput>;
