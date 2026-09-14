import { z } from "zod";
import { VectorStoreId } from "../primitives/branded.js";
import { Timestamp } from "../primitives/time.js";

export const VectorSourceKind = z.enum(["file", "folder"]);
export type VectorSourceKind = z.infer<typeof VectorSourceKind>;

const Pattern = z.string().trim().min(1).max(256);
const SourcePath = z.string().trim().min(1).max(4096);

export const VectorSourceDto = z.object({
  id: z.string().min(1),
  storeId: VectorStoreId,
  kind: VectorSourceKind,
  path: SourcePath,
  include: z.array(Pattern),
  exclude: z.array(Pattern),
  recursive: z.boolean(),
  createdAt: Timestamp,
});
export type VectorSourceDto = z.infer<typeof VectorSourceDto>;

export const VectorSourceRef = z.object({ id: z.string().min(1) });
export type VectorSourceRef = z.infer<typeof VectorSourceRef>;

export const AddVectorSourceInput = z.object({
  storeId: VectorStoreId,
  kind: VectorSourceKind.default("folder"),
  path: SourcePath,
  include: z.array(Pattern).max(64).default([]),
  exclude: z.array(Pattern).max(64).default([]),
  recursive: z.boolean().default(true),
});
export type AddVectorSourceInput = z.infer<typeof AddVectorSourceInput>;

export const PickVectorSourceInput = z.object({ kind: VectorSourceKind });
export type PickVectorSourceInput = z.infer<typeof PickVectorSourceInput>;

export const PickVectorSourceResult = z.object({ paths: z.array(z.string().min(1)) });
export type PickVectorSourceResult = z.infer<typeof PickVectorSourceResult>;

export const VectorIndexInput = z.object({
  storeId: VectorStoreId,
  full: z.boolean().default(false),
});
export type VectorIndexInput = z.infer<typeof VectorIndexInput>;

export const VectorIndexReportDto = z.object({
  storeId: VectorStoreId,
  files: z.number().int().nonnegative(),
  indexed: z.number().int().nonnegative(),
  unchanged: z.number().int().nonnegative(),
  removed: z.number().int().nonnegative(),
  skipped: z.number().int().nonnegative(),
  failed: z.number().int().nonnegative(),
  chunks: z.number().int().nonnegative(),
  cancelled: z.boolean(),
  notes: z.array(z.string()).max(200),
});
export type VectorIndexReportDto = z.infer<typeof VectorIndexReportDto>;
