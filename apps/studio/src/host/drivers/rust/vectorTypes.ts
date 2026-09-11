import { z } from "zod";

export const defaultVectorStore = { dimension: 1024, metric: "cosine" } as const;
export type VectorMetric = "cosine" | "l2" | "dot";
export type JsonValue =
  null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };

export interface VectorRow {
  id: string;
  vector: number[];
  payload: JsonValue;
  documentId: string;
  chunkIndex: number;
  path: string;
}

export const VectorStatsSchema = z.object({
  rowCount: z.number().int().nonnegative(),
  documentCount: z.number().int().nonnegative(),
  dimension: z.number().int().positive(),
  metric: z.enum(["cosine", "l2", "dot"]),
  onDiskBytes: z.number().int().nonnegative(),
  indexType: z.string(),
});

export const VectorHitSchema = z.object({
  id: z.string(),
  score: z.number().min(0).max(1),
  payload: z.json(),
  documentId: z.string(),
  chunkIndex: z.number().int().nonnegative(),
  path: z.string(),
});

export type VectorStats = z.infer<typeof VectorStatsSchema>;
export type VectorHit = z.infer<typeof VectorHitSchema>;

export interface VectorCorePort {
  removeVectorIndex(path: string): Promise<void>;
  createVectorIndex(path: string, dimension: number, metric?: VectorMetric): Promise<VectorStats>;
  openVectorIndex(path: string): Promise<VectorStats>;
  upsertVectors(path: string, rows: VectorRow[], signal?: AbortSignal): Promise<number>;
  searchVectors(
    path: string,
    vector: number[],
    k: number,
    minScore: number,
    filter?: string,
  ): Promise<VectorHit[]>;
  deleteVectorsByIds(path: string, ids: string[]): Promise<void>;
  deleteVectorsBySource(path: string, documentId: string): Promise<void>;
  vectorStats(path: string): Promise<VectorStats>;
}
