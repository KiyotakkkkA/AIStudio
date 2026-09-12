import {
  AppError,
  AppErrorCode,
  CreateVectorStoreInput,
  UpdateVectorStoreInput,
  VectorStoreDto,
  VectorSearchInput,
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

export interface VectorStoreServiceOptions {
  data: UnitOfWork;
  core: VectorCorePort;
  drivers: Pick<DriverSource, "ephemeralDriver">;
  directory: string;
  logger?: Logger;
  clock?: () => number;
}

export class VectorStoreService {
  private readonly busy = new Set<string>();
  constructor(private readonly options: VectorStoreServiceOptions) {}

  async create(input: CreateVectorStoreInput): Promise<VectorStoreDto> {
    const draft = CreateVectorStoreInput.parse(input);
    if (draft.backend !== "lancedb")
      throw new AppError(AppErrorCode.UNSUPPORTED_FORMAT, "Only LanceDB is supported");
    this.requireProvider(draft.embeddingProviderId);
    this.requireName(draft.name);
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
    options: { k?: number; minScore?: number } = {},
  ): Promise<VectorSearchHitDto[]> {
    return (await this.searchTimed(storeId, query, options)).hits;
  }

  searchTimed(
    storeId: string,
    query: string,
    options: { k?: number; minScore?: number } = {},
  ): Promise<VectorSearchResultDto> {
    const input = VectorSearchInput.parse({ storeId, query, ...options });
    return this.exclusive(storeId, async () => {
      const row = this.require(storeId);
      this.requireBackend(row);
      const provider = this.requireProvider(row.embeddingProviderId);
      const driver = await this.options.drivers.ephemeralDriver(provider);
      if (driver.embedding === null)
        throw new AppError(AppErrorCode.CONFLICT, "Provider does not support embeddings");
      const embeddingStart = performance.now();
      const vectors = await driver.embedding.embed(
        [input.query],
        row.embeddingModelId,
        AbortSignal.timeout((provider.settings.timeoutSeconds ?? 30) * 1000),
      );
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
      const searchStart = performance.now();
      const hits = await this.options.core.searchVectors(
        this.path(storeId),
        Array.from(vector),
        input.k,
        input.minScore,
      );
      return { hits, embeddingMs, searchMs: performance.now() - searchStart };
    });
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
