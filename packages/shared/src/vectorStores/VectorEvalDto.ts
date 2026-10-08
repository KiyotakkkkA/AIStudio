import { z } from "zod";
import { DocumentId, VectorStoreId } from "../primitives/branded.js";
import { Timestamp } from "../primitives/time.js";

export const VectorEvalCaseId = z.uuid();

export const VectorEvalCaseDto = z.object({
  id: VectorEvalCaseId,
  storeId: VectorStoreId,
  question: z.string().trim().min(1).max(2000),
  expectedDocumentIds: z.array(DocumentId).min(1).max(20),
  createdAt: Timestamp,
});
export type VectorEvalCaseDto = z.infer<typeof VectorEvalCaseDto>;

export const AddVectorEvalCaseInput = VectorEvalCaseDto.pick({
  storeId: true,
  question: true,
  expectedDocumentIds: true,
});
export type AddVectorEvalCaseInput = z.infer<typeof AddVectorEvalCaseInput>;

export const VectorEvalCaseRef = z.object({ storeId: VectorStoreId, id: VectorEvalCaseId });
export type VectorEvalCaseRef = z.infer<typeof VectorEvalCaseRef>;

export const VectorEvalRunInput = z.object({
  storeId: VectorStoreId,
  k: z.number().int().min(1).max(100).default(10),
});
export type VectorEvalRunInput = z.infer<typeof VectorEvalRunInput>;

const Ratio = z.number().min(0).max(1);

export const VectorEvalMetricsDto = z.object({
  cases: z.number().int().nonnegative(),
  hitAt1: Ratio,
  hitAt5: Ratio,
  recallAtK: Ratio,
  mrr: Ratio,
});
export type VectorEvalMetricsDto = z.infer<typeof VectorEvalMetricsDto>;

export const VectorEvalCaseResultDto = z.object({
  caseId: VectorEvalCaseId,
  rank: z.number().int().min(1).nullable(),
  found: z.number().int().nonnegative(),
  expected: z.number().int().nonnegative(),
  error: z.string().optional(),
});
export type VectorEvalCaseResultDto = z.infer<typeof VectorEvalCaseResultDto>;

export const VectorEvalMode = z.enum(["vector", "rerank"]);
export type VectorEvalMode = z.infer<typeof VectorEvalMode>;

export const VectorEvalModeResultDto = z.object({
  mode: VectorEvalMode,
  metrics: VectorEvalMetricsDto,
  cases: z.array(VectorEvalCaseResultDto),
  fallbacks: z.number().int().nonnegative().default(0),
});
export type VectorEvalModeResultDto = z.infer<typeof VectorEvalModeResultDto>;

export const VectorEvalRunDto = z.object({
  storeId: VectorStoreId,
  k: z.number().int().min(1),
  ranAt: Timestamp,
  durationMs: z.number().nonnegative(),
  modes: z.array(VectorEvalModeResultDto).min(1),
});
export type VectorEvalRunDto = z.infer<typeof VectorEvalRunDto>;

export const VectorEvalSetDto = z.object({
  cases: z.array(VectorEvalCaseDto),
  lastRun: VectorEvalRunDto.nullable(),
});
export type VectorEvalSetDto = z.infer<typeof VectorEvalSetDto>;
