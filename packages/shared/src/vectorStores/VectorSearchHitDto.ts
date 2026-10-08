import { z } from "zod";
import { VectorStoreId } from "../primitives/branded.js";
import { Json } from "../primitives/json.js";

export const VectorSearchInput = z.object({
  storeId: VectorStoreId,
  query: z.string().trim().min(1).max(100000),
  k: z.number().int().min(1).max(1000).default(10),
  minScore: z.number().min(0).max(1).default(0),
});
export type VectorSearchInput = z.infer<typeof VectorSearchInput>;
export const VectorSearchHitDto = z.object({
  id: z.string(),
  score: z.number().min(0).max(1),
  payload: Json,
  documentId: z.string(),
  chunkIndex: z.number().int().nonnegative(),
  path: z.string(),
  vectorScore: z.number().min(0).max(1).optional(),
  rerankScore: z.number().min(0).max(1).optional(),
});
export type VectorSearchHitDto = z.infer<typeof VectorSearchHitDto>;

export const VectorRerankOutcome = z.enum(["off", "applied", "fallback"]);
export type VectorRerankOutcome = z.infer<typeof VectorRerankOutcome>;

export const VectorSearchResultDto = z.object({
  hits: z.array(VectorSearchHitDto),
  embeddingMs: z.number().nonnegative(),
  searchMs: z.number().nonnegative(),
  rerankMs: z.number().nonnegative().optional(),
  rerank: VectorRerankOutcome.optional(),
});
export type VectorSearchResultDto = z.infer<typeof VectorSearchResultDto>;
