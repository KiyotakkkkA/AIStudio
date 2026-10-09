import { z } from "zod";
import { VectorStoreId } from "../primitives/branded.js";
import { Json } from "../primitives/json.js";
import { VectorSearchMode } from "./VectorStoreDto.js";

export const VectorSearchInput = z.object({
  storeId: VectorStoreId.describe("UUID of the vector store, as returned by vector.list"),
  query: z.string().trim().min(1).max(100000).describe("Natural-language search query"),
  k: z.number().int().min(1).max(1000).default(10).describe("Number of fragments to return"),
  minScore: z.number().min(0).max(1).default(0).describe("Drop fragments scoring below this"),
  mode: VectorSearchMode.optional().describe(
    "hybrid: vectors plus keywords; vector: vectors only. Omit to use the store's own setting",
  ),
});

export const VectorStoreListEntryDto = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string(),
  documents: z.number().int().nonnegative(),
  status: z.string(),
  searchMode: z.string(),
});
export type VectorStoreListEntryDto = z.infer<typeof VectorStoreListEntryDto>;
export type VectorSearchInput = z.infer<typeof VectorSearchInput>;
export const VectorHitProvenance = z.enum(["vector", "keyword", "both"]);
export type VectorHitProvenance = z.infer<typeof VectorHitProvenance>;

export const VectorSearchHitDto = z.object({
  id: z.string(),
  score: z.number().min(0).max(1),
  payload: Json,
  documentId: z.string(),
  chunkIndex: z.number().int().nonnegative(),
  path: z.string(),
  text: z.string().default(""),
  section: z.string().default(""),
  page: z.number().int().positive().nullable().default(null),
  vectorScore: z.number().min(0).max(1).optional(),
  rerankScore: z.number().min(0).max(1).optional(),
  keywordScore: z.number().nonnegative().optional(),
  fusedScore: z.number().nonnegative().optional(),
  provenance: VectorHitProvenance.optional(),
});
export type VectorSearchHitDto = z.infer<typeof VectorSearchHitDto>;

export function belowRelevanceFloor(
  hit: Pick<VectorSearchHitDto, "score" | "provenance">,
  floor: number,
): boolean {
  return hit.score < floor && hit.provenance !== "keyword";
}

export const VectorRerankOutcome = z.enum(["off", "applied", "fallback"]);
export type VectorRerankOutcome = z.infer<typeof VectorRerankOutcome>;

export const VectorSearchResultDto = z.object({
  hits: z.array(VectorSearchHitDto),
  embeddingMs: z.number().nonnegative(),
  searchMs: z.number().nonnegative(),
  rerankMs: z.number().nonnegative().optional(),
  rerank: VectorRerankOutcome.optional(),
  mode: VectorSearchMode.optional(),
});

export const VectorChunksInput = z.object({
  storeId: VectorStoreId,
  documentId: z.string().min(1),
  indices: z.array(z.number().int().nonnegative()).max(200),
});
export type VectorChunksInput = z.infer<typeof VectorChunksInput>;

export const VectorChunkDto = VectorSearchHitDto.pick({
  id: true,
  payload: true,
  documentId: true,
  chunkIndex: true,
  path: true,
  text: true,
  section: true,
  page: true,
});
export type VectorChunkDto = z.infer<typeof VectorChunkDto>;
export type VectorSearchResultDto = z.infer<typeof VectorSearchResultDto>;
