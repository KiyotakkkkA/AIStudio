import { afterEach, beforeEach, expect, test, vi } from "vitest";
import {
  AppErrorCode,
  contract,
  CreateVectorStoreInput,
  type VectorSearchResultDto,
  type VectorStoreDto,
} from "@zvs/shared";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { FakeVectorCore } from "../../../test/helpers/FakeVectorCore.ts";
import { createFakeDriver } from "../../../test/helpers/FakeDriver.ts";
import { createId } from "../src/host/platform/ids.ts";
import {
  VectorStoreService,
  type VectorSearchOptions,
} from "../src/host/services/VectorStoreService.ts";
import { VectorEvalService } from "../src/host/services/VectorEvalService.ts";
import { createVectorStoreHandlers } from "../src/host/ipc/vectorStores.ts";

let db: TemporaryDatabase;
let stores: VectorStoreService;
let evaluation: VectorEvalService;
let store: VectorStoreDto;
let documents: Record<"cats" | "dogs" | "birds", string>;

beforeEach(async () => {
  db = temporaryDatabase();
  const provider = db.client.repositories.providers.create({
    name: "Embedding",
    kind: "ollama",
    baseUrl: "http://localhost:11434",
    capabilities: ["embedding"],
    createdAt: 1,
    updatedAt: 1,
  });
  stores = new VectorStoreService({
    data: db.client,
    core: new FakeVectorCore(),
    drivers: { ephemeralDriver: () => Promise.resolve(createFakeDriver()) },
    directory: db.directory,
    clock: () => 1000,
  });
  store = await stores.create(
    CreateVectorStoreInput.parse({
      name: "Knowledge",
      embeddingProviderId: provider.id,
      embeddingModelId: "embed-model",
      dimension: 3,
    }),
  );
  const record = (name: string) =>
    db.client.repositories.vectorDocuments.recordIndexed({
      id: createId(),
      storeId: store.id,
      sourcePath: `C:/docs/${name}.md`,
      contentHash: name,
      chunkCount: 1,
      bytes: 10,
      indexedAt: 1,
    }).id;
  documents = { cats: record("cats"), dogs: record("dogs"), birds: record("birds") };
  evaluation = new VectorEvalService({ data: db.client, stores, clock: () => 5000 });
});
afterEach(() => db.dispose());

function result(order: readonly string[], rerank?: VectorSearchResultDto["rerank"]) {
  return {
    hits: order.map((documentId, index) => ({
      id: `${documentId}:${String(index)}`,
      score: 1 - index / 10,
      payload: null,
      documentId,
      chunkIndex: 0,
      path: documentId,
    })),
    embeddingMs: 1,
    searchMs: 1,
    ...(rerank === undefined ? {} : { rerank }),
  };
}

test("cases are written, listed in order and removed per store", () => {
  const first = evaluation.add({
    storeId: store.id,
    question: "Who purrs?",
    expectedDocumentIds: [documents.cats, documents.cats] as never,
  });
  evaluation.add({
    storeId: store.id,
    question: "Who barks?",
    expectedDocumentIds: [documents.dogs] as never,
  });
  expect(first.expectedDocumentIds).toEqual([documents.cats]);
  expect(evaluation.list(store.id).cases.map((entry) => entry.question)).toEqual([
    "Who purrs?",
    "Who barks?",
  ]);
  expect(evaluation.list(store.id).lastRun).toBeNull();

  evaluation.remove({ storeId: store.id, id: first.id });
  expect(evaluation.list(store.id).cases).toHaveLength(1);
  expect(() => evaluation.remove({ storeId: store.id, id: first.id })).toThrow(
    expect.objectContaining({ code: AppErrorCode.NOT_FOUND }),
  );
});

test("a case naming a document outside the store is refused", () => {
  expect(() =>
    evaluation.add({
      storeId: store.id,
      question: "Who?",
      expectedDocumentIds: [createId()] as never,
    }),
  ).toThrow(expect.objectContaining({ code: AppErrorCode.VALIDATION_FAILED }));
});

test("running with nothing to check says so", async () => {
  await expect(evaluation.run({ storeId: store.id, k: 10 })).rejects.toMatchObject({
    code: AppErrorCode.VALIDATION_FAILED,
  });
});

test("a run without reranking reports vector-only metrics and is kept with the store", async () => {
  evaluation.add({
    storeId: store.id,
    question: "cats",
    expectedDocumentIds: [documents.cats] as never,
  });
  evaluation.add({
    storeId: store.id,
    question: "dogs",
    expectedDocumentIds: [documents.dogs] as never,
  });
  const search = vi
    .spyOn(stores, "searchTimed")
    .mockImplementation((_id: string, query: string) =>
      Promise.resolve(
        query === "cats"
          ? result([documents.birds, documents.cats])
          : result([documents.dogs, documents.dogs]),
      ),
    );

  const run = await evaluation.run({ storeId: store.id, k: 10 });

  expect(search).toHaveBeenCalledTimes(2);
  expect(search.mock.calls[0]?.[2]).toEqual({ k: 10, minScore: 0, rerank: false });
  expect(run.modes.map((mode) => mode.mode)).toEqual(["vector"]);
  expect(run.modes[0]?.cases.map((entry) => entry.rank)).toEqual([2, 1]);
  expect(run.modes[0]?.metrics).toMatchObject({ cases: 2, hitAt1: 0.5, hitAt5: 1, mrr: 0.75 });
  expect(run.ranAt).toBe(5000);
  expect(evaluation.list(store.id).lastRun).toEqual(run);
});

test("a reranking store is measured both ways so the gain is visible", async () => {
  await stores.update({
    id: store.id,
    rerank: { enabled: true, modelRef: "curated:model:bge-reranker-v2-m3-f16", candidates: 50 },
  });
  evaluation.add({
    storeId: store.id,
    question: "cats",
    expectedDocumentIds: [documents.cats] as never,
  });
  vi.spyOn(stores, "searchTimed").mockImplementation(
    (_id: string, _query: string, options: VectorSearchOptions = {}) =>
      Promise.resolve(
        options.rerank === false
          ? result([documents.dogs, documents.birds, documents.cats])
          : result([documents.cats, documents.dogs], "applied"),
      ),
  );

  const run = await evaluation.run({ storeId: store.id, k: 5 });

  expect(run.modes.map((mode) => [mode.mode, mode.metrics.mrr])).toEqual([
    ["vector", 1 / 3],
    ["rerank", 1],
  ]);
  expect(run.modes[1]?.fallbacks).toBe(0);
});

test("fallbacks and failed searches are reported per case instead of sinking the run", async () => {
  await stores.update({
    id: store.id,
    rerank: { enabled: true, modelRef: "curated:model:bge-reranker-v2-m3-f16", candidates: 50 },
  });
  evaluation.add({
    storeId: store.id,
    question: "cats",
    expectedDocumentIds: [documents.cats] as never,
  });
  evaluation.add({
    storeId: store.id,
    question: "boom",
    expectedDocumentIds: [documents.dogs] as never,
  });
  vi.spyOn(stores, "searchTimed").mockImplementation(
    (_id: string, query: string, options: VectorSearchOptions = {}) =>
      query === "boom"
        ? Promise.reject(new Error("embedding provider is down"))
        : Promise.resolve(
            result([documents.cats], options.rerank === false ? undefined : "fallback"),
          ),
  );

  const run = await evaluation.run({ storeId: store.id, k: 5 });
  const reranked = run.modes.find((mode) => mode.mode === "rerank")!;

  expect(reranked.fallbacks).toBe(1);
  expect(reranked.cases[1]).toMatchObject({
    rank: null,
    found: 0,
    expected: 1,
    error: "embedding provider is down",
  });
  expect(reranked.metrics.mrr).toBe(0.5);
});

test("cases and the kept result go with the store", async () => {
  evaluation.add({
    storeId: store.id,
    question: "cats",
    expectedDocumentIds: [documents.cats] as never,
  });
  await stores.remove(store.id);
  expect(db.client.repositories.vectorEvalCases.listByStore(store.id)).toEqual([]);
});

test("the eval channels round-trip through the contract", async () => {
  const handlers = createVectorStoreHandlers({ service: stores, evaluation });
  const added = await handlers["vectorStores.eval.add"]({
    storeId: store.id,
    question: "cats",
    expectedDocumentIds: [documents.cats] as never,
  });
  expect(contract["vectorStores.eval.add"].output.parse(added)).toEqual(added);
  const set = await handlers["vectorStores.eval.list"]({ id: store.id });
  expect(contract["vectorStores.eval.list"].output.parse(set).cases).toHaveLength(1);
  expect(await handlers["vectorStores.eval.remove"]({ storeId: store.id, id: added.id })).toEqual({
    id: added.id,
    removed: true,
  });
  expect(() =>
    createVectorStoreHandlers({ service: stores })["vectorStores.eval.list"]({ id: store.id }),
  ).toThrow("Проверка качества недоступна");
});
