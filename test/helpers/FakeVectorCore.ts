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
  upserts = 0;
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
    this.rows.set(path, []);
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
  async upsertVectors(path: string, rows: VectorRow[], signal?: AbortSignal) {
    if (signal?.aborted === true)
      throw new AppError(AppErrorCode.RUN_CANCELLED, "Операция отменена");
    await this.vectorStats(path);
    this.upserts += 1;
    const incoming = new Set(rows.map((row) => row.id));
    const kept = (this.rows.get(path) ?? []).filter((row) => !incoming.has(row.id));
    this.write(path, [...kept, ...rows]);
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
  async deleteVectorsByIds(path: string, ids: string[]) {
    const gone = new Set(ids);
    this.write(
      path,
      (this.rows.get(path) ?? []).filter((row) => !gone.has(row.id)),
    );
  }
  async deleteVectorsBySource(path: string, documentId: string) {
    this.write(
      path,
      (this.rows.get(path) ?? []).filter((row) => row.documentId !== documentId),
    );
  }
  rowsFor(path: string, documentId: string): VectorRow[] {
    return (this.rows.get(path) ?? []).filter((row) => row.documentId === documentId);
  }
  private write(path: string, rows: VectorRow[]): void {
    this.rows.set(path, rows);
    const stats = this.tables.get(path);
    if (!stats) return;
    this.tables.set(path, {
      ...stats,
      rowCount: rows.length,
      documentCount: new Set(rows.map((row) => row.documentId)).size,
    });
  }
}
