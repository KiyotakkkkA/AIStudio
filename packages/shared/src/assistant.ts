import { z } from "zod";

export const SUMMARIZER_MODELS_MAX = 5;
export const SUMMARIZER_MODEL_ID_MAX = 200;
export const AUTO_COMPACT_PERCENT = 80;

export const SummarizerModelDto = z.object({
  providerId: z.string().min(1).max(SUMMARIZER_MODEL_ID_MAX),
  modelId: z.string().min(1).max(SUMMARIZER_MODEL_ID_MAX),
});
export type SummarizerModelDto = z.infer<typeof SummarizerModelDto>;

export const AssistantPreferencesDto = z.object({
  autoCompact: z.boolean(),
  recaps: z.boolean(),
  summarizerModels: z
    .array(SummarizerModelDto)
    .max(SUMMARIZER_MODELS_MAX)
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
});
export type AssistantPreferencesDto = z.infer<typeof AssistantPreferencesDto>;
