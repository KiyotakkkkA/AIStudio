import { AppError, AppErrorCode } from "@zvs/shared";
import type {
  VectorChunk,
  VectorCorePort,
  VectorMetric,
  VectorStats,
  VectorRow,
  VectorHit,
  VectorSearchRequest,
} from "../../apps/studio/src/host/drivers/rust/vectorTypes.ts";

export class FakeVectorCore implements VectorCorePort {
  readonly tables = new Map<string, VectorStats>();
  readonly rows = new Map<string, VectorRow[]>();
  readonly searches: { path: string; request: VectorSearchRequest }[] = [];
  readonly maintained: { path: string; annThreshold: number }[] = [];
  upserts = 0;
  async createVectorIndex(path: string, dimension: number, metric: VectorMetric = "cosine") {
    const stats: VectorStats = {
      dimension,
      metric,
      rowCount: 0,
      documentCount: 0,
      onDiskBytes: 100,
      indexType: "FLAT",
      schemaVersion: 2,
      keywordIndex: true,
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
    const stats = await this.vectorStats(path);
    if (stats.schemaVersion < 2)
      throw new AppError(AppErrorCode.VALIDATION_FAILED, "Vector index must be fully re-indexed");
    this.upserts += 1;
    const incoming = new Set(rows.map((row) => row.id));
    const kept = (this.rows.get(path) ?? []).filter((row) => !incoming.has(row.id));
    this.write(path, [...kept, ...rows]);
    return rows.length;
  }
  async searchVectors(path: string, request?: VectorSearchRequest): Promise<VectorHit[]> {
    await this.vectorStats(path);
    if (request !== undefined) this.searches.push({ path, request });
    const scope = request?.documentIds === undefined ? undefined : new Set(request.documentIds);
    return (this.rows.get(path) ?? [])
      .filter((row) => scope === undefined || scope.has(row.documentId))
      .slice(0, request?.k ?? Number.POSITIVE_INFINITY)
      .map((row) => ({
        ...chunkOf(row),
        score: 1,
        ...(request?.mode === "hybrid"
          ? { provenance: "vector" as const, fusedScore: 1 / 61 }
          : {}),
      }));
  }
  async vectorChunks(path: string, documentId: string, indices: number[]): Promise<VectorChunk[]> {
    await this.vectorStats(path);
    const wanted = new Set(indices);
    return (this.rows.get(path) ?? [])
      .filter((row) => row.documentId === documentId && wanted.has(row.chunkIndex))
      .sort((left, right) => left.chunkIndex - right.chunkIndex)
      .map(chunkOf);
  }
  async maintainVectorIndex(path: string, annThreshold: number) {
    const stats = await this.vectorStats(path);
    this.maintained.push({ path, annThreshold });
    return stats;
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
  setSchemaVersion(path: string, schemaVersion: number): void {
    const stats = this.tables.get(path);
    if (stats) this.tables.set(path, { ...stats, schemaVersion, keywordIndex: schemaVersion >= 2 });
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

function chunkOf(row: VectorRow): VectorChunk {
  return {
    id: row.id,
    documentId: row.documentId,
    chunkIndex: row.chunkIndex,
    path: row.path,
    payload: row.payload,
    text: row.text,
    section: row.section,
    page: row.page,
  };
}
