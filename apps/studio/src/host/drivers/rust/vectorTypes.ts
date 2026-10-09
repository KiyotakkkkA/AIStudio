import { z } from "zod";

export const defaultVectorStore = { dimension: 1024, metric: "cosine" } as const;
export type VectorMetric = "cosine" | "l2" | "dot";
type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };

export interface VectorRow {
  id: string;
  vector: number[];
  payload: JsonValue;
  documentId: string;
  chunkIndex: number;
  path: string;
  text: string;
  section: string;
  page: number | null;
}

export type VectorSearchMode = "hybrid" | "vector";

export interface VectorSearchRequest {
  vector: number[];
  k: number;
  minScore: number;
  mode?: VectorSearchMode;
  query?: string;
  documentIds?: string[];
}

export const VectorStatsSchema = z.object({
  rowCount: z.number().int().nonnegative(),
  documentCount: z.number().int().nonnegative(),
  dimension: z.number().int().positive(),
  metric: z.enum(["cosine", "l2", "dot"]),
  onDiskBytes: z.number().int().nonnegative(),
  indexType: z.string(),
  schemaVersion: z.number().int().positive().default(1),
  keywordIndex: z.boolean().default(false),
});

export const VectorChunkSchema = z.object({
  id: z.string(),
  payload: z.json(),
  documentId: z.string(),
  chunkIndex: z.number().int().nonnegative(),
  path: z.string(),
  text: z.string().default(""),
  section: z.string().default(""),
  page: z.number().int().positive().nullable().default(null),
});

export const VectorHitSchema = VectorChunkSchema.extend({
  score: z.number().min(0).max(1),
  keywordScore: z.number().nonnegative().optional(),
  fusedScore: z.number().nonnegative().optional(),
  provenance: z.enum(["vector", "keyword", "both"]).optional(),
});

export type VectorStats = z.infer<typeof VectorStatsSchema>;
export type VectorHit = z.infer<typeof VectorHitSchema>;
export type VectorChunk = z.infer<typeof VectorChunkSchema>;

export interface VectorCorePort {
  removeVectorIndex(path: string): Promise<void>;
  createVectorIndex(path: string, dimension: number, metric?: VectorMetric): Promise<VectorStats>;
  openVectorIndex(path: string): Promise<VectorStats>;
  upsertVectors(path: string, rows: VectorRow[], signal?: AbortSignal): Promise<number>;
  searchVectors(path: string, request: VectorSearchRequest): Promise<VectorHit[]>;
  vectorChunks(path: string, documentId: string, indices: number[]): Promise<VectorChunk[]>;
  maintainVectorIndex(path: string, annThreshold: number): Promise<VectorStats>;
  deleteVectorsByIds(path: string, ids: string[]): Promise<void>;
  deleteVectorsBySource(path: string, documentId: string): Promise<void>;
  vectorStats(path: string): Promise<VectorStats>;
}
