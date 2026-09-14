import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import {
  AddVectorSourceInput,
  AppError,
  AppErrorCode,
  VectorDocumentDto,
  VectorIndexInput,
  VectorIndexReportDto,
  VectorSourceDto,
} from "@zvs/shared";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import type {
  VectorDocumentEntity,
  VectorSourceEntity,
  VectorStoreEntity,
} from "../data/schema/index.ts";
import type { EmbeddingDriver } from "../drivers/ai/ports.ts";
import type { RustCorePort, TextChunk } from "../drivers/rust/ports.ts";
import type { VectorCorePort, VectorRow } from "../drivers/rust/vectorTypes.ts";
import type { Logger } from "../platform/logger.ts";
import { createId } from "../platform/ids.ts";
import type { DriverSource } from "../services/ProviderService.ts";
import type { VectorStoreService } from "../services/VectorStoreService.ts";
import { createExtractorRegistry, type ExtractorRegistry } from "./extraction.ts";
import { walkSources, type DiscoveredFile, type WalkLimits } from "./walk.ts";

export interface IndexProgress {
  done: number;
  total: number;
  message?: string;
}

export interface IndexContext {
  signal?: AbortSignal;
  onProgress?: (progress: IndexProgress) => void;
}

export interface IndexingServiceOptions {
  data: UnitOfWork;
  core: RustCorePort & VectorCorePort;
  drivers: Pick<DriverSource, "ephemeralDriver">;
  stores: VectorStoreService;
  extractors?: ExtractorRegistry;
  limits?: WalkLimits;
  logger?: Logger;
  clock?: () => number;
  sleep?: (ms: number, signal?: AbortSignal) => Promise<void>;
  batchSize?: number;
  concurrency?: number;
  maxAttempts?: number;
  backoffMs?: number;
  progressIntervalMs?: number;
  readFile?: (path: string) => Promise<Uint8Array>;
}

interface Tally {
  files: number;
  indexed: number;
  unchanged: number;
  removed: number;
  skipped: number;
  failed: number;
  chunks: number;
  notes: string[];
}

const BYTES_PER_TOKEN = 6;

export function estimateChunks(bytes: number, chunkSize: number, chunkOverlap: number): number {
  const tokens = Math.max(1, Math.ceil(bytes / BYTES_PER_TOKEN));
  const stride = Math.max(1, chunkSize - chunkOverlap);
  return Math.max(1, Math.ceil(tokens / stride));
}

function defaultSleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolveSleep, reject) => {
    if (signal?.aborted) {
      reject(new AppError(AppErrorCode.RUN_CANCELLED, "Индексация отменена"));
      return;
    }
    const timer = setTimeout(() => {
      signal?.removeEventListener("abort", onAbort);
      resolveSleep();
    }, ms);
    const onAbort = () => {
      clearTimeout(timer);
      reject(new AppError(AppErrorCode.RUN_CANCELLED, "Индексация отменена"));
    };
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

export class IndexingService {
  private readonly running = new Set<string>();
  private lastReportAt = 0;
  private readonly extractors: ExtractorRegistry;

  constructor(private readonly options: IndexingServiceOptions) {
    this.extractors = options.extractors ?? createExtractorRegistry();
  }

  get registry(): ExtractorRegistry {
    return this.extractors;
  }

  listSources(storeId: string): VectorSourceDto[] {
    this.options.stores.requireStore(storeId);
    return this.sources.listByStore(storeId).map(toSourceDto);
  }

  addSource(raw: AddVectorSourceInput): VectorSourceDto {
    const input = AddVectorSourceInput.parse(raw);
    this.options.stores.requireStore(input.storeId);
    return toSourceDto(
      this.sources.add({
        id: createId(),
        storeId: input.storeId,
        kind: input.kind,
        path: resolve(input.path),
        include: [...input.include],
        exclude: [...input.exclude],
        recursive: input.recursive,
        createdAt: this.now(),
      }),
    );
  }

  removeSource(id: string): void {
    if (!this.sources.findById(id))
      throw new AppError(AppErrorCode.NOT_FOUND, "Источник не найден");
    this.sources.remove(id);
  }

  listDocuments(storeId: string): VectorDocumentDto[] {
    this.options.stores.requireStore(storeId);
    return this.documents.listByStore(storeId).map((row) => VectorDocumentDto.parse(row));
  }

  async removeDocument(storeId: string, documentId: string): Promise<void> {
    const row = this.documents.findById(documentId);
    if (!row || row.storeId !== storeId)
      throw new AppError(AppErrorCode.NOT_FOUND, "Документ не найден");
    await this.options.stores.withExclusiveStore(storeId, async (store) => {
      await this.options.core.deleteVectorsBySource(this.path(store), row.id);
      this.documents.remove(row.id);
    });
    await this.options.stores.reconcile(storeId);
  }

  async index(raw: VectorIndexInput, context: IndexContext = {}): Promise<VectorIndexReportDto> {
    const input = VectorIndexInput.parse(raw);
    const store = this.options.stores.requireStore(input.storeId);
    if (store.backend !== "lancedb")
      throw new AppError(AppErrorCode.UNSUPPORTED_FORMAT, "Поддерживается только LanceDB");
    if (this.running.has(store.id))
      throw new AppError(AppErrorCode.CONFLICT, "Индексация этого хранилища уже выполняется");
    this.running.add(store.id);
    try {
      return await this.pipeline(store, input.full, context);
    } finally {
      this.running.delete(store.id);
    }
  }

  private async pipeline(
    store: VectorStoreEntity,
    full: boolean,
    context: IndexContext,
  ): Promise<VectorIndexReportDto> {
    const signal = context.signal;
    const sources = this.sources.listByStore(store.id);
    if (sources.length === 0)
      throw new AppError(AppErrorCode.CONFLICT, "У хранилища нет источников для индексации");
    const embedding = await this.embedding(store, signal);
    const walked = await walkSources(sources, this.options.limits, signal);
    const tally: Tally = {
      files: walked.files.length,
      indexed: 0,
      unchanged: 0,
      removed: 0,
      skipped: 0,
      failed: 0,
      chunks: 0,
      notes: [...walked.notes],
    };
    const known = new Map(this.documents.listByStore(store.id).map((row) => [row.sourcePath, row]));
    const discovered = new Map(walked.files.map((file) => [file.path, file]));

    let pending = walked.files.reduce(
      (sum, file) => sum + estimateChunks(file.bytes, store.chunkSize, store.chunkOverlap),
      0,
    );
    const report = (message?: string, force = false) =>
      this.report(context, tally, pending, message, force);
    report(`Найдено файлов: ${String(walked.files.length)}`, true);

    let cancelled = aborted(signal);
    for (const [path, document] of known) {
      if (discovered.has(path)) continue;
      if (cancelled) break;
      try {
        await this.forget(store, document);
        tally.removed += 1;
      } catch (error) {
        tally.failed += 1;
        this.note(tally, `Не удалось удалить ${path}: ${describe(error)}`);
      }
    }

    for (const file of walked.files) {
      if (aborted(signal)) {
        cancelled = true;
        break;
      }
      const estimate = estimateChunks(file.bytes, store.chunkSize, store.chunkOverlap);
      try {
        const outcome = await this.ingest(store, file, known.get(file.path), full, embedding, {
          signal,
          onBatch: (embedded) => {
            tally.chunks += embedded;
            report();
          },
        });
        if (outcome.status === "unchanged") tally.unchanged += 1;
        else if (outcome.status === "skipped") {
          tally.skipped += 1;
          this.note(tally, `Пропущен ${file.path}: ${outcome.reason}`);
        } else tally.indexed += 1;
        pending -= estimate;
        report(`${file.path}`, true);
      } catch (error) {
        pending -= estimate;
        if (isCancellation(error) || aborted(signal)) {
          cancelled = true;
          break;
        }
        tally.failed += 1;
        this.note(tally, `Ошибка ${file.path}: ${describe(error)}`);
        this.options.logger?.log("warn", "indexing", "Could not index a file", {
          storeId: store.id,
          path: file.path,
          error: String(error),
        });
        report(`${file.path}`, true);
      }
    }

    pending = 0;
    report(
      cancelled
        ? `Отменено: обработано ${String(tally.indexed)} из ${String(tally.files)} файлов`
        : `Готово: ${String(tally.indexed)} проиндексировано, ${String(tally.unchanged)} без изменений`,
      true,
    );

    if (tally.indexed > 0 || tally.removed > 0)
      this.stores.update(store.id, { lastIndexedAt: this.now(), updatedAt: this.now() });
    await this.options.stores.reconcile(store.id).catch((error: unknown) => {
      this.options.logger?.log("warn", "indexing", "Could not reconcile after indexing", {
        storeId: store.id,
        error: String(error),
      });
    });

    return VectorIndexReportDto.parse({
      storeId: store.id,
      files: tally.files,
      indexed: tally.indexed,
      unchanged: tally.unchanged,
      removed: tally.removed,
      skipped: tally.skipped,
      failed: tally.failed,
      chunks: tally.chunks,
      cancelled,
      notes: tally.notes,
    });
  }

  private async ingest(
    store: VectorStoreEntity,
    file: DiscoveredFile,
    known: VectorDocumentEntity | undefined,
    full: boolean,
    embedding: EmbeddingDriver,
    context: { signal?: AbortSignal; onBatch: (embedded: number) => void },
  ): Promise<{ status: "indexed" | "unchanged" | "skipped"; reason?: string }> {
    if (!this.extractors.supports(file.path))
      return { status: "skipped", reason: this.extractors.reason(file.path) };
    const bytes = await (this.options.readFile ?? defaultRead)(file.path);
    const contentHash = await this.options.core.hash(bytes);
    if (!full && known && known.contentHash === contentHash) return { status: "unchanged" };
    const text = await this.extractors.extract({ path: file.path, bytes });
    const chunks = await this.options.core.chunk(text, {
      size: store.chunkSize,
      overlap: store.chunkOverlap,
    });
    const documentId = known?.id ?? createId();
    const vectors = await this.embed(store, embedding, chunks, file, documentId, context);
    await this.options.stores.withExclusiveStore(store.id, async (row) => {
      const path = this.path(row);
      if (known) await this.options.core.deleteVectorsBySource(path, documentId);
      if (vectors.length > 0) await this.options.core.upsertVectors(path, vectors, context.signal);
      this.documents.recordIndexed({
        id: documentId,
        storeId: row.id,
        sourcePath: file.path,
        contentHash,
        chunkCount: vectors.length,
        bytes: bytes.byteLength,
        indexedAt: this.now(),
      });
    });
    return { status: "indexed" };
  }

  private async embed(
    store: VectorStoreEntity,
    embedding: EmbeddingDriver,
    chunks: readonly TextChunk[],
    file: DiscoveredFile,
    documentId: string,
    context: { signal?: AbortSignal; onBatch: (embedded: number) => void },
  ): Promise<VectorRow[]> {
    const size = this.options.batchSize ?? 32;
    const batches: { start: number; chunks: TextChunk[] }[] = [];
    for (let start = 0; start < chunks.length; start += size)
      batches.push({ start, chunks: chunks.slice(start, start + size) });
    const rows: VectorRow[] = [];
    const concurrency = Math.max(1, Math.min(this.options.concurrency ?? 2, batches.length));
    let next = 0;
    const worker = async (): Promise<void> => {
      for (;;) {
        if (aborted(context.signal))
          throw new AppError(AppErrorCode.RUN_CANCELLED, "Индексация отменена");
        const batch = batches[next++];
        if (!batch) return;
        const embedded = await this.embedBatch(
          embedding,
          store,
          batch.chunks.map((chunk) => chunk.text),
          context.signal,
        );
        for (const [offset, vector] of embedded.entries()) {
          const index = batch.start + offset;
          const chunk = batch.chunks[offset]!;
          rows.push({
            id: `${documentId}:${String(index)}`,
            vector: Array.from(vector),
            documentId,
            chunkIndex: index,
            path: file.path,
            payload: {
              text: chunk.text,
              path: file.path,
              chunkIndex: index,
              chunkCount: chunks.length,
              byteStart: chunk.byteStart,
              byteEnd: chunk.byteEnd,
              tokenCount: chunk.tokenCount,
            },
          });
        }
        context.onBatch(embedded.length);
      }
    };
    await Promise.all(Array.from({ length: concurrency }, worker));
    return rows.sort((a, b) => a.chunkIndex - b.chunkIndex);
  }

  private async embedBatch(
    embedding: EmbeddingDriver,
    store: VectorStoreEntity,
    texts: readonly string[],
    signal?: AbortSignal,
  ): Promise<Float32Array[]> {
    const attempts = Math.max(1, this.options.maxAttempts ?? 5);
    const base = this.options.backoffMs ?? 500;
    const sleep = this.options.sleep ?? defaultSleep;
    for (let attempt = 1; ; attempt += 1) {
      signal?.throwIfAborted();
      try {
        const vectors = await embedding.embed(
          texts,
          store.embeddingModelId,
          signal ?? new AbortController().signal,
        );
        if (vectors.length !== texts.length)
          throw new AppError(
            AppErrorCode.VALIDATION_FAILED,
            "Провайдер вернул неожиданное число векторов",
          );
        for (const vector of vectors)
          if (vector.length !== store.dimension || !vector.every(Number.isFinite))
            throw new AppError(
              AppErrorCode.VALIDATION_FAILED,
              "Размерность эмбеддинга не совпадает с хранилищем",
            );
        return [...vectors];
      } catch (error) {
        const limited = error instanceof AppError && error.code === AppErrorCode.RATE_LIMITED;
        if (!limited || attempt >= attempts || aborted(signal)) throw error;
        const delay = base * 2 ** (attempt - 1);
        this.options.logger?.log("info", "indexing", "Backing off after a rate limit", {
          storeId: store.id,
          attempt,
          delayMs: delay,
        });
        await sleep(delay, signal);
      }
    }
  }

  private async forget(store: VectorStoreEntity, document: VectorDocumentEntity): Promise<void> {
    await this.options.stores.withExclusiveStore(store.id, async (row) => {
      await this.options.core.deleteVectorsBySource(this.path(row), document.id);
      this.documents.remove(document.id);
    });
  }

  private async embedding(
    store: VectorStoreEntity,
    signal?: AbortSignal,
  ): Promise<EmbeddingDriver> {
    const provider = this.options.data.repositories.providers.findById(store.embeddingProviderId);
    if (!provider) throw new AppError(AppErrorCode.NOT_FOUND, "Провайдер эмбеддингов не найден");
    if (!provider.enabled || !provider.capEmbedding)
      throw new AppError(
        AppErrorCode.CONFLICT,
        "Провайдер эмбеддингов должен быть включён и поддерживать эмбеддинги",
      );
    signal?.throwIfAborted();
    const driver = await this.options.drivers.ephemeralDriver(provider);
    if (driver.embedding === null)
      throw new AppError(AppErrorCode.CONFLICT, "Провайдер не поддерживает эмбеддинги");
    return driver.embedding;
  }

  private report(
    context: IndexContext,
    tally: Tally,
    pending: number,
    message?: string,
    force = false,
  ): void {
    if (!context.onProgress) return;
    const at = this.now();
    const interval = this.options.progressIntervalMs ?? 250;
    if (!force && at - this.lastReportAt < interval) return;
    this.lastReportAt = at;
    context.onProgress({
      done: tally.chunks,
      total: Math.max(tally.chunks, tally.chunks + pending),
      ...(message === undefined ? {} : { message }),
    });
  }

  private note(tally: Tally, message: string): void {
    if (tally.notes.length < 200) tally.notes.push(message);
  }

  private path(store: VectorStoreEntity): string {
    return this.options.stores.storePath(store.id);
  }
  private now(): number {
    return (this.options.clock ?? Date.now)();
  }
  private get sources() {
    return this.options.data.repositories.vectorSources;
  }
  private get documents() {
    return this.options.data.repositories.vectorDocuments;
  }
  private get stores() {
    return this.options.data.repositories.vectorStores;
  }
}

function toSourceDto(row: VectorSourceEntity): VectorSourceDto {
  return VectorSourceDto.parse(row);
}

function defaultRead(path: string): Promise<Uint8Array> {
  return readFile(path);
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function aborted(signal?: AbortSignal): boolean {
  return signal?.aborted === true;
}

function isCancellation(error: unknown): boolean {
  return (
    (error instanceof AppError && error.code === AppErrorCode.RUN_CANCELLED) ||
    (error instanceof Error && error.name === "AbortError")
  );
}
