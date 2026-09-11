import { AppError, AppErrorCode } from "@zvs/shared";
import type {
  VectorCorePort,
  VectorMetric,
  VectorStats,
  VectorRow,
  VectorHit,
} from "../../apps/studio/src/host/drivers/rust/vectorTypes.ts";

export class FakeVectorCore implements VectorCorePort {
  readonly tables = new Map<string, VectorStats>();
  readonly rows = new Map<string, VectorRow[]>();
  async createVectorIndex(path: string, dimension: number, metric: VectorMetric = "cosine") {
    const stats = {
      dimension,
      metric,
      rowCount: 0,
      documentCount: 0,
      onDiskBytes: 100,
      indexType: "FLAT",
    };
    this.tables.set(path, stats);
    return stats;
  }
  async vectorStats(path: string) {
    const stats = this.tables.get(path);
    if (!stats) throw new AppError(AppErrorCode.NOT_FOUND, "Missing table");
    return stats;
  }
  openVectorIndex(path: string) {
    return this.vectorStats(path);
  }
  async removeVectorIndex(path: string) {
    this.tables.delete(path);
    this.rows.delete(path);
  }
  async upsertVectors(path: string, rows: VectorRow[]) {
    const stats = await this.vectorStats(path);
    this.rows.set(path, rows);
    this.tables.set(path, {
      ...stats,
      rowCount: rows.length,
      documentCount: new Set(rows.map((row) => row.documentId)).size,
    });
    return rows.length;
  }
  async searchVectors(path: string): Promise<VectorHit[]> {
    await this.vectorStats(path);
    return (this.rows.get(path) ?? []).map((row) => ({
      id: row.id,
      documentId: row.documentId,
      chunkIndex: row.chunkIndex,
      path: row.path,
      payload: row.payload,
      score: 1,
    }));
  }
  async deleteVectorsByIds() {}
  async deleteVectorsBySource() {}
}
