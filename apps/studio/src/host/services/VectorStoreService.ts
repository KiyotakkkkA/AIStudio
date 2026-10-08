import {
  AppError,
  AppErrorCode,
  CreateVectorStoreInput,
  UpdateVectorStoreInput,
  VectorStoreDto,
  VectorSearchInput,
  type VectorOcrConfig,
  type VectorRerankConfig,
  type VectorSearchHitDto,
  type VectorSearchResultDto,
} from "@zvs/shared";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import type { VectorStoreEntity } from "../data/schema/index.ts";
import type { VectorCorePort, VectorStats } from "../drivers/rust/vectorTypes.ts";
import type { DriverSource } from "./ProviderService.ts";
import type { Logger } from "../platform/logger.ts";
import { createId } from "../platform/ids.ts";
import { vectorStorePath } from "../platform/paths.ts";
import { deriveVectorHealth } from "./vectorHealth.ts";
import { applyRerank, MAX_RERANK_QUERY_CHARS, passageOf, type RerankerSource } from "./rerank.ts";

export interface VectorSearchOptions {
  k?: number;
  minScore?: number;
  rerank?: boolean;
}

export const DEFAULT_RERANK_TIMEOUT_MS = 20_000;

export interface VectorStoreServiceOptions {
  data: UnitOfWork;
  core: VectorCorePort;
  drivers: Pick<DriverSource, "ephemeralDriver">;
  directory: string;
  logger?: Logger;
  clock?: () => number;
  rerankers?: RerankerSource;
  rerankTimeoutMs?: number;
}

export class VectorStoreService {
  private readonly busy = new Set<string>();
  private rerankers: RerankerSource | undefined;
  constructor(private readonly options: VectorStoreServiceOptions) {
    this.rerankers = options.rerankers;
  }

  attachRerankers(source: RerankerSource): void {
    this.rerankers = source;
  }

  async create(input: CreateVectorStoreInput): Promise<VectorStoreDto> {
    const draft = CreateVectorStoreInput.parse(input);
    if (draft.backend !== "lancedb")
      throw new AppError(AppErrorCode.UNSUPPORTED_FORMAT, "Only LanceDB is supported");
    this.requireProvider(draft.embeddingProviderId);
    this.requireName(draft.name);
    this.requireAdvanced(draft.rerank, draft.ocr);
    const now = this.now();
    const row = this.stores.create({ ...draft, id: createId(), createdAt: now, updatedAt: now });
    return this.exclusive(row.id, async () => {
      try {
        const stats = await this.options.core.createVectorIndex(
          this.path(row.id),
          row.dimension,
          row.metric,
        );
        if (stats.dimension !== row.dimension || stats.metric !== row.metric) {
          throw new AppError(
            AppErrorCode.NATIVE_ERROR,
            "Created index configuration does not match the store",
          );
        }
        return this.repair(row, stats);
      } catch (cause) {
        try {
          await this.options.core.removeVectorIndex(this.path(row.id));
        } catch (cleanupError) {
          this.stores.update(row.id, { status: "broken", updatedAt: this.now() });
          this.options.logger?.log("error", "vectorStores", "Could not clean up failed creation", {
            storeId: row.id,
            error: String(cleanupError),
          });
          throw new AppError(
            AppErrorCode.NATIVE_ERROR,
            "Store creation and cleanup failed; metadata retained for removal",
            { cause },
          );
        }
        this.stores.remove(row.id);
        throw cause;
      }
    });
  }

  async list(): Promise<VectorStoreDto[]> {
    return Promise.all(
      this.stores.list().map(async (row) => {
        if (this.busy.has(row.id)) return this.toDto(row);
        const observed = await this.observe(row);
        return this.toDto({ ...row, status: observed.health });
      }),
    );
  }

  get(id: string): Promise<VectorStoreDto> {
    return this.reconcile(id);
  }

  update(input: UpdateVectorStoreInput): Promise<VectorStoreDto> {
    const { id, ...patch } = UpdateVectorStoreInput.parse(input);
    return this.exclusive(id, async () => {
      const row = this.require(id);
      if ((patch.chunkOverlap ?? row.chunkOverlap) >= (patch.chunkSize ?? row.chunkSize)) {
        throw new AppError(
          AppErrorCode.VALIDATION_FAILED,
          "Chunk overlap must be smaller than chunk size",
        );
      }
      if (
        (patch.chunkSize !== undefined && patch.chunkSize !== row.chunkSize) ||
        (patch.chunkOverlap !== undefined && patch.chunkOverlap !== row.chunkOverlap)
      ) {
        if (
          row.vectors > 0 ||
          row.lastIndexedAt !== null ||
          this.options.data.repositories.vectorDocuments.listByStore(id).length > 0
        ) {
          throw new AppError(
            AppErrorCode.CONFLICT,
            "Chunk settings can only change before indexing",
          );
        }
        const observed = await this.observe(row);
        if (observed.health === "broken" || (observed.stats?.rowCount ?? 0) > 0) {
          throw new AppError(AppErrorCode.CONFLICT, "Cannot change chunk settings for this index");
        }
      }
      if (patch.name !== undefined) this.requireName(patch.name, id);
      this.requireAdvanced(patch.rerank ?? row.rerank, patch.ocr ?? row.ocr);
      const updated = this.stores.update(id, { ...patch, updatedAt: this.now() })!;
      const observed = await this.observe(updated);
      return this.toDto({ ...updated, status: observed.health });
    });
  }

  remove(id: string): Promise<void> {
    return this.exclusive(id, async () => {
      const row = this.require(id);
      this.requireBackend(row);
      try {
        await this.options.core.removeVectorIndex(this.path(id));
      } catch (cause) {
        this.options.logger?.log("error", "vectorStores", "Could not remove vector store", {
          storeId: id,
          error: String(cause),
        });
        throw new AppError(
          AppErrorCode.NATIVE_ERROR,
          "Could not remove vector store; metadata retained",
          { cause },
        );
      }
      this.stores.remove(id);
    });
  }

  /**
   * Empties the store without destroying it: the table is dropped and recreated with the same
   * dimension and metric, and the document rows go with it. Sources, chunking and the OCR
   * settings are untouched, so the next index refills it from the same configuration.
   *
   * Recreating beats deleting row by row — a full corpus is millions of vectors, and a fresh
   * table also reclaims the space on disk that a delete would only mark as free.
   */
  clear(id: string): Promise<VectorStoreDto> {
    return this.exclusive(id, async () => {
      const row = this.require(id);
      this.requireBackend(row);
      const path = this.path(id);
      try {
        await this.options.core.removeVectorIndex(path);
        await this.options.core.createVectorIndex(path, row.dimension, row.metric);
      } catch (cause) {
        // A store that lost its table and could not get a new one is broken, and says so rather
        // than reporting a successful reset.
        this.stores.update(id, { status: "broken", updatedAt: this.now() });
        this.options.logger?.log("error", "vectorStores", "Could not reset the vector store", {
          storeId: id,
          error: String(cause),
        });
        throw new AppError(AppErrorCode.NATIVE_ERROR, "Не удалось очистить хранилище", { cause });
      }
      const removed = this.options.data.repositories.vectorDocuments.removeByStore(id);
      const updated = this.stores.update(id, {
        documents: 0,
        vectors: 0,
        bytes: 0,
        lastIndexedAt: null,
        status: "pending",
        updatedAt: this.now(),
      })!;
      this.options.logger?.log("info", "vectorStores", "Reset a vector store", {
        storeId: id,
        documents: removed,
      });
      return this.toDto(updated);
    });
  }

  reconcile(id: string): Promise<VectorStoreDto> {
    return this.exclusive(id, async () => {
      const row = this.require(id);
      const observed = await this.observe(row);
      if (observed.stats !== null && observed.health !== "broken")
        return this.repair(row, observed.stats);
      return this.toDto(
        this.stores.update(id, { status: observed.health, updatedAt: this.now() })!,
      );
    });
  }

  async search(
    storeId: string,
    query: string,
    options: VectorSearchOptions = {},
    signal?: AbortSignal,
  ): Promise<VectorSearchHitDto[]> {
    return (await this.searchTimed(storeId, query, options, signal)).hits;
  }

  searchTimed(
    storeId: string,
    query: string,
    options: VectorSearchOptions = {},
    signal?: AbortSignal,
  ): Promise<VectorSearchResultDto> {
    const { rerank: allowRerank = true, ...search } = options;
    const input = VectorSearchInput.parse({ storeId, query, ...search });
    return this.exclusive(storeId, async () => {
      signal?.throwIfAborted();
      const row = this.require(storeId);
      this.requireBackend(row);
      const provider = this.requireProvider(row.embeddingProviderId);
      const driver = await this.options.drivers.ephemeralDriver(provider);
      signal?.throwIfAborted();
      if (driver.embedding === null)
        throw new AppError(AppErrorCode.CONFLICT, "Provider does not support embeddings");
      const embeddingStart = performance.now();
      const timeout = AbortSignal.timeout((provider.settings.timeoutSeconds ?? 30) * 1000);
      const vectors = await driver.embedding.embed(
        [input.query],
        row.embeddingModelId,
        signal ? AbortSignal.any([signal, timeout]) : timeout,
      );
      signal?.throwIfAborted();
      const embeddingMs = performance.now() - embeddingStart;
      const vector = vectors[0];
      if (
        vectors.length !== 1 ||
        vector === undefined ||
        vector.length !== row.dimension ||
        !vector.every(Number.isFinite)
      ) {
        throw new AppError(
          AppErrorCode.VALIDATION_FAILED,
          "Embedding dimension does not match the store",
        );
      }
      const reranking = allowRerank && row.rerank.enabled;
      const searchStart = performance.now();
      const hits = await this.options.core.searchVectors(
        this.path(storeId),
        Array.from(vector),
        reranking ? Math.max(input.k, row.rerank.candidates) : input.k,
        reranking ? 0 : input.minScore,
      );
      signal?.throwIfAborted();
      const searchMs = performance.now() - searchStart;
      if (!reranking) return { hits, embeddingMs, searchMs };
      const rerankStart = performance.now();
      const reranked = await this.rerankHits(row, input.query, hits, signal);
      const rerankMs = performance.now() - rerankStart;
      const ordered = reranked ?? hits;
      return {
        hits: ordered.filter((hit) => hit.score >= input.minScore).slice(0, input.k),
        embeddingMs,
        searchMs,
        rerankMs,
        rerank: reranked === undefined ? ("fallback" as const) : ("applied" as const),
      };
    });
  }

  private async rerankHits(
    row: VectorStoreEntity,
    query: string,
    hits: readonly VectorSearchHitDto[],
    signal: AbortSignal | undefined,
  ): Promise<VectorSearchHitDto[] | undefined> {
    if (hits.length === 0) return [];
    if (this.rerankers === undefined) {
      this.options.logger?.log("warn", "vectorStores", "Reranker unavailable, using vector order", {
        storeId: row.id,
        modelRef: row.rerank.modelRef,
      });
      return undefined;
    }
    const timeout = AbortSignal.timeout(this.options.rerankTimeoutMs ?? DEFAULT_RERANK_TIMEOUT_MS);
    try {
      const scores = await this.rerankers(row.rerank.modelRef).rerank(
        query.slice(0, MAX_RERANK_QUERY_CHARS),
        hits.map(passageOf),
        signal ? AbortSignal.any([signal, timeout]) : timeout,
      );
      if (scores.length !== hits.length || !scores.every(Number.isFinite))
        throw new AppError(AppErrorCode.VALIDATION_FAILED, "Reranker returned a malformed answer", {
          details: { expected: hits.length, received: scores.length },
        });
      return applyRerank(hits, scores);
    } catch (error) {
      if (signal?.aborted === true) throw error;
      this.options.logger?.log("warn", "vectorStores", "Reranking failed, using vector order", {
        storeId: row.id,
        modelRef: row.rerank.modelRef,
        timedOut: timeout.aborted,
        error: String(error),
      });
      return undefined;
    }
  }

  withExclusiveStore<T>(id: string, work: (row: VectorStoreEntity) => Promise<T>): Promise<T> {
    return this.exclusive(id, () => work(this.require(id)));
  }

  requireStore(id: string): VectorStoreEntity {
    return this.require(id);
  }

  storePath(id: string): string {
    return this.path(id);
  }

  private async observe(row: VectorStoreEntity) {
    try {
      this.requireBackend(row);
      const stats = await this.options.core.vectorStats(this.path(row.id));
      return { stats, health: deriveVectorHealth(row, stats) };
    } catch (error) {
      const missing = error instanceof AppError && error.code === AppErrorCode.NOT_FOUND;
      if (!missing)
        this.options.logger?.log("warn", "vectorStores", "Could not inspect vector store", {
          storeId: row.id,
          error: String(error),
        });
      return { stats: null, health: deriveVectorHealth(row, null, !missing) };
    }
  }

  private repair(row: VectorStoreEntity, stats: VectorStats): VectorStoreDto {
    const now = this.now();
    return this.toDto(
      this.stores.update(row.id, {
        documents: stats.documentCount,
        vectors: stats.rowCount,
        bytes: stats.onDiskBytes,
        indexType: stats.indexType,
        status: "healthy",
        tableCreatedAt: row.tableCreatedAt ?? now,
        updatedAt: now,
      })!,
    );
  }

  private requireProvider(id: string) {
    const provider = this.options.data.repositories.providers.findById(id);
    if (!provider) throw new AppError(AppErrorCode.NOT_FOUND, "Embedding provider not found");
    if (!provider.enabled || !provider.capEmbedding)
      throw new AppError(
        AppErrorCode.CONFLICT,
        "Embedding provider must be enabled and support embeddings",
      );
    return provider;
  }
  /** An enabled stage without a model is a setting that would silently do nothing. */
  private requireAdvanced(rerank: VectorRerankConfig, ocr: VectorOcrConfig) {
    if (rerank.enabled && rerank.modelRef.trim() === "")
      throw new AppError(AppErrorCode.VALIDATION_FAILED, "Выберите модель переранжирования");
    if (ocr.enabled && ocr.modelRef.trim() === "")
      throw new AppError(AppErrorCode.VALIDATION_FAILED, "Выберите модель OCR");
  }
  private requireBackend(row: VectorStoreEntity) {
    if (row.backend !== "lancedb")
      throw new AppError(AppErrorCode.UNSUPPORTED_FORMAT, "Only LanceDB is supported");
  }
  private requireName(name: string, id?: string) {
    const existing = this.stores.findByName(name);
    if (existing && existing.id !== id)
      throw new AppError(AppErrorCode.CONFLICT, "Vector store name already exists");
  }
  private require(id: string) {
    const row = this.stores.findById(id);
    if (!row) throw new AppError(AppErrorCode.NOT_FOUND, "Vector store not found");
    return row;
  }
  private async exclusive<T>(id: string, work: () => Promise<T>): Promise<T> {
    if (this.busy.has(id)) throw new AppError(AppErrorCode.CONFLICT, "Vector store is busy");
    this.busy.add(id);
    try {
      return await work();
    } finally {
      this.busy.delete(id);
    }
  }
  private toDto(row: VectorStoreEntity): VectorStoreDto {
    return VectorStoreDto.parse(row);
  }
  private path(id: string) {
    return vectorStorePath(this.options.directory, id);
  }
  private now() {
    return (this.options.clock ?? Date.now)();
  }
  private get stores() {
    return this.options.data.repositories.vectorStores;
  }
}
