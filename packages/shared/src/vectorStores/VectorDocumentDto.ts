import { z } from "zod";
import { DocumentId, VectorStoreId } from "../primitives/branded.js";
import { Timestamp } from "../primitives/time.js";

export const VectorDocumentDto = z.object({
  id: DocumentId,
  storeId: VectorStoreId,
  sourcePath: z.string().min(1),
  contentHash: z.string().min(1),
  chunkCount: z.number().int().nonnegative(),
  bytes: z.number().int().nonnegative(),
  indexedAt: Timestamp,
});
export type VectorDocumentDto = z.infer<typeof VectorDocumentDto>;
