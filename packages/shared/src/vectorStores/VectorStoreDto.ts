import { z } from "zod";
import { ProviderId, VectorStoreId } from "../primitives/branded.js";
import { Timestamp } from "../primitives/time.js";

export const VectorBackend = z.enum(["lancedb", "qdrant", "pgvector"]);
export const VectorMetric = z.enum(["cosine", "l2", "dot"]);
export const VectorHealth = z.enum(["healthy", "stale", "pending", "broken"]);
export type VectorHealth = z.infer<typeof VectorHealth>;

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
});
export type UpdateVectorStoreInput = z.infer<typeof UpdateVectorStoreInput>;
