import { z } from "zod";
import { RunId, StepId } from "../primitives/branded.js";

export const StepStatus = z.enum(["running", "succeeded", "failed", "cancelled", "abandoned"]);
export const StepDto = z.object({
  id: StepId,
  runId: RunId,
  nodeId: z.string(),
  type: z.string(),
  status: StepStatus,
  input: z.json(),
  output: z.json().optional(),
  error: z.string().optional(),
  startedAt: z.number().int(),
  finishedAt: z.number().int().optional(),
  attempt: z.number().int().positive(),
});
export type StepDto = z.infer<typeof StepDto>;
