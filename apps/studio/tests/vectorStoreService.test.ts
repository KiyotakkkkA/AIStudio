import { afterEach, beforeEach, expect, test, vi } from "vitest";
import {
  AppError,
  AppErrorCode,
  CreateVectorStoreInput,
  VectorStoreDto,
  contract,
} from "@zvs/shared";
import { join } from "node:path";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { FakeVectorCore } from "../../../test/helpers/FakeVectorCore.ts";
import { createFakeDriver } from "../../../test/helpers/FakeDriver.ts";
import { createId } from "../src/host/platform/ids.ts";
import { VectorStoreService } from "../src/host/services/VectorStoreService.ts";
import { deriveVectorHealth } from "../src/host/services/vectorHealth.ts";
import { createVectorStoreHandlers } from "../src/host/ipc/vectorStores.ts";

let db: TemporaryDatabase;
let core: FakeVectorCore;
let service: VectorStoreService;
let input: CreateVectorStoreInput;
const embed = vi.fn(async () => [new Float32Array([1, 0, 0])]);
const driverSource = vi.fn(async () => ({
  ...createFakeDriver(),
  embedding: {
    embed,
    dimensions: () => 3,
    listModels: async () => [],
    capabilities: () => createFakeDriver().text.capabilities(),
  },
}));
const log = vi.fn();

beforeEach(() => {
  db = temporaryDatabase();
  core = new FakeVectorCore();
  const provider = db.client.repositories.providers.create({
    name: "Embedding",
    kind: "ollama",
    baseUrl: "http://localhost:11434",
    capabilities: ["embedding"],
    createdAt: 1,
    updatedAt: 1,
  });
  input = CreateVectorStoreInput.parse({
    name: "Knowledge",
    embeddingProviderId: provider.id,
    embeddingModelId: "embed-model",
    dimension: 3,
  });
  service = new VectorStoreService({
    data: db.client,
    core,
    drivers: { ephemeralDriver: driverSource },
    directory: db.directory,
    logger: { log, close() {} },
    clock: () => 1000,
  });
  vi.clearAllMocks();
});
afterEach(() => db.dispose());

test("timed search measures embedding and native search separately", async () => {
  const store = await service.create(input);
  const timer = vi.spyOn(performance, "now");
  timer
    .mockReturnValueOnce(100)
    .mockReturnValueOnce(118)
    .mockReturnValueOnce(120)
    .mockReturnValueOnce(165);
  try {
    const result = await service.searchTimed(store.id, "hello");
    expect(result).toEqual({ hits: [], embeddingMs: 18, searchMs: 45 });
  } finally {
    timer.mockRestore();
  }
});

test("creation persists the pending row before native work and rolls back partial creation", async () => {
  vi.spyOn(core, "createVectorIndex").mockImplementation(async () => {
    expect(db.client.repositories.vectorStores.list()[0]?.status).toBe("pending");
    throw new Error("failed");
  });
  const cleanup = vi.spyOn(core, "removeVectorIndex");
  await expect(service.create(input)).rejects.toThrow("failed");
  expect(cleanup).toHaveBeenCalledOnce();
  expect(db.client.repositories.vectorStores.list()).toEqual([]);
});

test("failed creation retains recoverable metadata if native cleanup also fails", async () => {
  vi.spyOn(core, "createVectorIndex").mockRejectedValue(new Error("failed"));
  vi.spyOn(core, "removeVectorIndex").mockRejectedValue(new Error("locked"));
  await expect(service.create(input)).rejects.toMatchObject({ code: AppErrorCode.NATIVE_ERROR });
  expect(db.client.repositories.vectorStores.list()[0]?.status).toBe("broken");
  expect(log).toHaveBeenCalled();
});

test("delete removes native storage first, cascades documents, and restricts provider deletion", async () => {
  const store = await service.create(input);
  const documents = db.client.repositories.vectorDocuments;
  documents.recordIndexed({
    id: createId(),
    storeId: store.id,
    sourcePath: "a.md",
    contentHash: "abc",
    chunkCount: 2,
    bytes: 12,
    indexedAt: 2,
  });
  expect(() => db.client.repositories.providers.remove(store.embeddingProviderId)).toThrow();
  const remove = core.removeVectorIndex.bind(core);
  vi.spyOn(core, "removeVectorIndex").mockImplementation(async (path) => {
    expect(db.client.repositories.vectorStores.findById(store.id)).toBeDefined();
    await remove(path);
  });
  await service.remove(store.id);
  expect(core.tables.size).toBe(0);
  expect(documents.listByStore(store.id)).toEqual([]);
  expect(await service.list()).toEqual([]);
  expect(() => db.client.repositories.providers.remove(store.embeddingProviderId)).not.toThrow();
});

test("failed deletion logs and preserves metadata; a later removal succeeds", async () => {
  const store = await service.create(input);
  vi.spyOn(core, "removeVectorIndex").mockRejectedValueOnce(new Error("locked"));
  await expect(service.remove(store.id)).rejects.toMatchObject({ code: AppErrorCode.NATIVE_ERROR });
  expect(db.client.repositories.vectorStores.findById(store.id)).toBeDefined();
  expect(core.tables.size).toBe(1);
  expect(log).toHaveBeenCalled();
  await service.remove(store.id);
});

test("list reports drift; opening and explicit reconciliation repair all native counts", async () => {
  const store = await service.create(input);
  const path = join(db.directory, store.id);
  core.tables.set(path, {
    ...core.tables.get(path)!,
    rowCount: 5,
    documentCount: 2,
    onDiskBytes: 456,
  });
  expect((await service.list())[0]?.status).toBe("stale");
  expect(await service.get(store.id)).toMatchObject({
    status: "healthy",
    documents: 2,
    vectors: 5,
    bytes: 456,
    lastIndexedAt: null,
  });
  core.tables.set(path, {
    ...core.tables.get(path)!,
    rowCount: 1,
    documentCount: 1,
    onDiskBytes: 100,
  });
  expect(await service.reconcile(store.id)).toMatchObject({ documents: 1, vectors: 1, bytes: 100 });
  core.tables.delete(path);
  expect((await service.get(store.id)).status).toBe("broken");
  await service.remove(store.id);
});

test("health distinguishes uncreated, lost, mismatched and corrupt tables", async () => {
  const row = {
    tableCreatedAt: null,
    documents: 0,
    vectors: 0,
    bytes: 10,
    dimension: 3,
    metric: "cosine" as const,
  };
  const stats = {
    documentCount: 0,
    rowCount: 0,
    onDiskBytes: 10,
    dimension: 3,
    metric: "cosine" as const,
    indexType: "FLAT",
  };
  expect(deriveVectorHealth(row, null)).toBe("pending");
  expect(deriveVectorHealth({ ...row, tableCreatedAt: 1 }, null)).toBe("broken");
  expect(deriveVectorHealth(row, stats)).toBe("healthy");
  expect(deriveVectorHealth(row, { ...stats, rowCount: 1 })).toBe("stale");
  expect(deriveVectorHealth(row, { ...stats, documentCount: 1 })).toBe("stale");
  expect(deriveVectorHealth(row, { ...stats, dimension: 4 })).toBe("broken");
  expect(deriveVectorHealth(row, null, true)).toBe("broken");
  const pending = db.client.repositories.vectorStores.create({
    ...input,
    id: createId(),
    createdAt: 1,
    updatedAt: 1,
  });
  expect((await service.get(pending.id)).status).toBe("pending");
  vi.spyOn(core, "vectorStats").mockRejectedValue(
    new AppError(AppErrorCode.NATIVE_ERROR, "corrupt"),
  );
  expect((await service.get(pending.id)).status).toBe("broken");
});

test("search embeds with the saved provider and external model then passes the vector to Rust", async () => {
  const store = await service.create(input);
  const search = vi.spyOn(core, "searchVectors");
  await service.search(store.id, "question", { k: 5, minScore: 0.3 });
  expect(driverSource.mock.calls[0]).toEqual([
    db.client.repositories.providers.findById(store.embeddingProviderId),
  ]);
  expect(embed).toHaveBeenCalledWith(["question"], "embed-model", expect.any(AbortSignal));
  expect(search).toHaveBeenCalledWith(join(db.directory, store.id), [1, 0, 0], 5, 0.3);
  embed.mockResolvedValueOnce([new Float32Array([1])]);
  await expect(service.search(store.id, "question")).rejects.toMatchObject({
    code: AppErrorCode.VALIDATION_FAILED,
  });
  db.client.repositories.providers.update(store.embeddingProviderId, { enabled: false });
  await expect(service.search(store.id, "question")).rejects.toMatchObject({
    code: AppErrorCode.CONFLICT,
  });
});

test("document upsert retains identity and supports unchanged-file lookup", async () => {
  const store = await service.create(input);
  const repo = db.client.repositories.vectorDocuments;
  const draft = {
    id: createId(),
    storeId: store.id,
    sourcePath: "a.md",
    contentHash: "abc",
    chunkCount: 2,
    bytes: 12,
    indexedAt: 2,
  };
  repo.recordIndexed(draft);
  const changed = repo.recordIndexed({
    ...draft,
    id: createId(),
    contentHash: "def",
    chunkCount: 3,
  });
  expect(changed.id).toBe(draft.id);
  expect(repo.findBySource(store.id, "a.md")?.contentHash).toBe("def");
  expect(repo.listByStore(store.id)).toHaveLength(1);
  repo.removeBySource(store.id, "a.md");
  expect(repo.listByStore(store.id)).toEqual([]);
});

test("channels cover CRUD and reconciliation with validated DTOs and immutable index configuration", async () => {
  const handlers = createVectorStoreHandlers(service);
  const store = await handlers["vectorStores.create"](input);
  expect(VectorStoreDto.safeParse(store).success).toBe(true);
  expect(store).not.toHaveProperty("tableCreatedAt");
  expect(await handlers["vectorStores.list"](undefined)).toHaveLength(1);
  expect((await handlers["vectorStores.update"]({ id: store.id, name: "New" })).name).toBe("New");
  expect((await handlers["vectorStores.get"]({ id: store.id })).status).toBe("healthy");
  expect((await handlers["vectorStores.reconcile"]({ id: store.id })).status).toBe("healthy");
  expect(
    await handlers["vectorStores.search"]({ storeId: store.id, query: "q", k: 3, minScore: 0 }),
  ).toEqual([]);
  await expect(service.create({ ...input, name: "New" })).rejects.toMatchObject({
    code: AppErrorCode.CONFLICT,
  });
  await expect(service.create({ ...input, backend: "qdrant" })).rejects.toMatchObject({
    code: AppErrorCode.UNSUPPORTED_FORMAT,
  });
  await expect(service.update({ id: store.id, chunkOverlap: 999 })).rejects.toThrow();
  expect(
    contract["vectorStores.create"].input.safeParse({ ...input, chunkOverlap: 999 }).success,
  ).toBe(false);
  await handlers["vectorStores.remove"]({ id: store.id });
  await expect(service.get(store.id)).rejects.toMatchObject({ code: AppErrorCode.NOT_FOUND });
});

test("mutations cannot race active native operations", async () => {
  const store = await service.create(input);
  let release!: () => void;
  const holding = new Promise<void>((resolve) => {
    release = resolve;
  });
  vi.spyOn(core, "removeVectorIndex").mockImplementation(() => holding);
  const removing = service.remove(store.id);
  await expect(service.update({ id: store.id, name: "New" })).rejects.toMatchObject({
    code: AppErrorCode.CONFLICT,
  });
  await expect(service.reconcile(store.id)).rejects.toMatchObject({ code: AppErrorCode.CONFLICT });
  release();
  await removing;
});
