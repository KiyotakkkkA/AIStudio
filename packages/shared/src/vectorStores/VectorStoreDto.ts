import { z } from "zod";
import { ProviderId, VectorStoreId } from "../primitives/branded.js";
import { Timestamp } from "../primitives/time.js";

export const VectorBackend = z.enum(["lancedb", "qdrant", "pgvector"]);
export const VectorMetric = z.enum(["cosine", "l2", "dot"]);
export const VectorHealth = z.enum(["healthy", "stale", "pending", "broken"]);
export type VectorHealth = z.infer<typeof VectorHealth>;

/**
 * Reranking and OCR are optional stages around the embedding one: a cross-encoder that
 * reorders what the vector search returned, and a vision model that turns scanned pages into
 * text before chunking. Both reference an artefact installed from the Downloads catalogue by
 * its `ItemRef`, because neither is served by an embedding provider.
 */
export const VectorRerankConfig = z.object({
  enabled: z.boolean().default(false),
  modelRef: z.string().trim().max(256).default(""),
  /** How many hits the search pulls before the cross-encoder reorders them. */
  candidates: z.number().int().min(1).max(500).default(50),
});
export type VectorRerankConfig = z.infer<typeof VectorRerankConfig>;

export const VECTOR_OCR_LANGUAGES = ["auto", "rus", "eng", "rus+eng"] as const;
export const VectorOcrLanguage = z.enum(VECTOR_OCR_LANGUAGES);
export type VectorOcrLanguage = z.infer<typeof VectorOcrLanguage>;

export const VectorOcrConfig = z.object({
  enabled: z.boolean().default(false),
  modelRef: z.string().trim().max(256).default(""),
  language: VectorOcrLanguage.default("auto"),
  /** Below this much extracted text a page counts as scanned and is sent to the OCR model. */
  minCharsPerPage: z.number().int().min(0).max(100000).default(200),
});
export type VectorOcrConfig = z.infer<typeof VectorOcrConfig>;

export const VectorStoreDto = z.object({
  id: VectorStoreId,
  name: z.string().trim().min(1).max(128),
  description: z.string().max(4096),
  backend: VectorBackend,
  embeddingProviderId: ProviderId,
  embeddingModelId: z.string().trim().min(1).max(256),
  dimension: z.number().int().min(1).max(65536),
  metric: VectorMetric,
  chunkSize: z.number().int().min(1).max(1000000),
  chunkOverlap: z.number().int().nonnegative(),
  // Prefaulted rather than required: a row written before these settings existed still reads.
  rerank: VectorRerankConfig.prefault({}),
  ocr: VectorOcrConfig.prefault({}),
  indexType: z.string().min(1),
  status: VectorHealth,
  lastIndexedAt: Timestamp.nullable(),
  documents: z.number().int().nonnegative(),
  vectors: z.number().int().nonnegative(),
  bytes: z.number().int().nonnegative(),
  createdAt: Timestamp,
  updatedAt: Timestamp,
});
export type VectorStoreDto = z.infer<typeof VectorStoreDto>;

export const VectorStoreRef = z.object({ id: VectorStoreId });
export const CreateVectorStoreInput = VectorStoreDto.pick({
  name: true,
  embeddingProviderId: true,
  embeddingModelId: true,
})
  .extend({
    description: VectorStoreDto.shape.description.default(""),
    backend: VectorBackend.default("lancedb"),
    dimension: VectorStoreDto.shape.dimension.default(1024),
    metric: VectorMetric.default("cosine"),
    chunkSize: VectorStoreDto.shape.chunkSize.default(256),
    chunkOverlap: VectorStoreDto.shape.chunkOverlap.default(32),
    rerank: VectorRerankConfig.prefault({}),
    ocr: VectorOcrConfig.prefault({}),
    indexType: z.literal("FLAT").default("FLAT"),
  })
  .refine((input) => input.chunkOverlap < input.chunkSize, {
    message: "Chunk overlap must be smaller than chunk size",
    path: ["chunkOverlap"],
  });
export type CreateVectorStoreInput = z.infer<typeof CreateVectorStoreInput>;

export const UpdateVectorStoreInput = VectorStoreRef.extend({
  name: VectorStoreDto.shape.name.optional(),
  description: VectorStoreDto.shape.description.optional(),
  chunkSize: VectorStoreDto.shape.chunkSize.optional(),
  chunkOverlap: VectorStoreDto.shape.chunkOverlap.optional(),
  rerank: VectorRerankConfig.optional(),
  ocr: VectorOcrConfig.optional(),
});
export type UpdateVectorStoreInput = z.infer<typeof UpdateVectorStoreInput>;
