import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { CreateVectorStoreInput, type VectorStoreDto } from "@zvs/shared";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { FakeVectorCore } from "../../../test/helpers/FakeVectorCore.ts";
import { createFakeDriver } from "../../../test/helpers/FakeDriver.ts";
import type { VectorHit } from "../src/host/drivers/rust/vectorTypes.ts";
import { VectorStoreService } from "../src/host/services/VectorStoreService.ts";
import {
  applyRerank,
  localRerankers,
  MAX_RERANK_PASSAGE_CHARS,
  normaliseRerankScore,
  passageOf,
  type RerankPort,
} from "../src/host/services/rerank.ts";

let db: TemporaryDatabase;
let core: FakeVectorCore;
let service: VectorStoreService;
let input: CreateVectorStoreInput;
const log = vi.fn();
const rerank = vi.fn<RerankPort["rerank"]>();
const rerankers = vi.fn((modelRef: string): RerankPort => {
  void modelRef;
  return { rerank };
});

function hit(id: string, score: number, text: string, documentId = id): VectorHit {
  return {
    id,
    score,
    documentId,
    chunkIndex: 0,
    path: `${documentId}.md`,
    payload: { text, chunkCount: 1 },
  };
}

const CANDIDATES = [
  hit("a", 0.9, "about cats"),
  hit("b", 0.8, "about dogs"),
  hit("c", 0.7, "the answer"),
  hit("d", 0.2, "noise"),
];

function makeService(options: Partial<ConstructorParameters<typeof VectorStoreService>[0]> = {}) {
  return new VectorStoreService({
    data: db.client,
    core,
    drivers: {
      ephemeralDriver: () =>
        Promise.resolve({
          ...createFakeDriver(),
          embedding: {
            embed: () => Promise.resolve([new Float32Array([1, 0, 0])]),
            dimensions: () => 3,
            listModels: () => Promise.resolve([]),
            capabilities: () => createFakeDriver().text.capabilities(),
          },
        }),
    },
    directory: db.directory,
    logger: { log, close() {} },
    clock: () => 1000,
    rerankers,
    ...options,
  });
}

async function rerankingStore(candidates = 50): Promise<VectorStoreDto> {
  return service.create({
    ...input,
    rerank: { enabled: true, modelRef: "curated:model:bge-reranker-v2-m3-f16", candidates },
  });
}

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
  vi.clearAllMocks();
  vi.spyOn(core, "searchVectors").mockResolvedValue(CANDIDATES.map((entry) => ({ ...entry })));
  service = makeService();
});
afterEach(() => db.dispose());

test("logits are squashed into [0, 1] monotonically and without overflow", () => {
  expect(normaliseRerankScore(0)).toBe(0.5);
  expect(normaliseRerankScore(0.9417868852615356)).toBeCloseTo(0.7195, 3);
  expect(normaliseRerankScore(-7.699778079986572)).toBeLessThan(0.001);
  expect(normaliseRerankScore(1000)).toBe(1);
  expect(normaliseRerankScore(-1000)).toBe(0);
  expect(normaliseRerankScore(2)).toBeGreaterThan(normaliseRerankScore(1));
});

test("the passage is the chunk text, capped so one long chunk cannot fill the context", () => {
  expect(passageOf({ ...hit("a", 1, "text"), payload: "raw string" })).toBe("raw string");
  expect(passageOf(hit("a", 1, "x".repeat(10_000)))).toHaveLength(MAX_RERANK_PASSAGE_CHARS);
  expect(passageOf({ ...hit("a", 1, ""), payload: null })).toBe("a.md");
});

test("reranked hits keep both scores and use the rerank one as the score", () => {
  const reranked = applyRerank(CANDIDATES, [0, 0, 3, -3]);
  expect(reranked.map((entry) => entry.id)).toEqual(["c", "a", "b", "d"]);
  expect(reranked[0]).toMatchObject({ vectorScore: 0.7, rerankScore: normaliseRerankScore(3) });
  expect(reranked[0]?.score).toBe(reranked[0]?.rerankScore);
});

test("a store with reranking on fetches the candidates, reranks them and returns k", async () => {
  const store = await rerankingStore(30);
  rerank.mockResolvedValue([-2, -1, 4, 0]);

  const result = await service.searchTimed(store.id, "which one is the answer?", { k: 2 });

  expect(core.searchVectors).toHaveBeenCalledWith(expect.any(String), [1, 0, 0], 30, 0);
  expect(rerankers).toHaveBeenCalledWith("curated:model:bge-reranker-v2-m3-f16");
  expect(rerank.mock.calls[0]?.[0]).toBe("which one is the answer?");
  expect(rerank.mock.calls[0]?.[1]).toEqual(["about cats", "about dogs", "the answer", "noise"]);
  expect(result.rerank).toBe("applied");
  expect(result.rerankMs).toBeGreaterThanOrEqual(0);
  expect(result.hits.map((entry) => entry.id)).toEqual(["c", "d"]);
  expect(result.hits[0]).toMatchObject({ vectorScore: 0.7 });
  expect(result.hits[0]?.rerankScore).toBeCloseTo(normaliseRerankScore(4));
});

test("the floor applies to the rerank score once reranking ran", async () => {
  const store = await rerankingStore();
  rerank.mockResolvedValue([-5, -5, 5, -5]);
  const hits = await service.search(store.id, "q", { k: 10, minScore: 0.5 });
  expect(hits.map((entry) => entry.id)).toEqual(["c"]);
});

test("k larger than the candidate count still fetches k", async () => {
  const store = await rerankingStore(2);
  rerank.mockResolvedValue([0, 0, 0, 0]);
  await service.search(store.id, "q", { k: 4 });
  expect(core.searchVectors).toHaveBeenCalledWith(expect.any(String), [1, 0, 0], 4, 0);
});

test("a store with reranking off never touches the reranker", async () => {
  const store = await service.create(input);
  const result = await service.searchTimed(store.id, "q", { k: 3, minScore: 0.1 });
  expect(core.searchVectors).toHaveBeenCalledWith(expect.any(String), [1, 0, 0], 3, 0.1);
  expect(rerankers).not.toHaveBeenCalled();
  expect(result.rerank).toBeUndefined();
  expect(result.rerankMs).toBeUndefined();
  expect(result.hits[0]?.rerankScore).toBeUndefined();
});

test("the caller can ask for vector order on a reranking store", async () => {
  const store = await rerankingStore();
  const result = await service.searchTimed(store.id, "q", { k: 2, rerank: false });
  expect(rerank).not.toHaveBeenCalled();
  expect(result.rerank).toBeUndefined();
  expect(result.hits.map((entry) => entry.id)).toEqual(["a", "b", "c", "d"]);
});

test("a crashing reranker degrades to vector order with a warning", async () => {
  const store = await rerankingStore();
  rerank.mockRejectedValue(new Error("connection reset"));

  const result = await service.searchTimed(store.id, "q", { k: 2, minScore: 0.75 });

  expect(result.rerank).toBe("fallback");
  expect(result.hits.map((entry) => entry.id)).toEqual(["a", "b"]);
  expect(result.hits[0]?.rerankScore).toBeUndefined();
  expect(log).toHaveBeenCalledWith(
    "warn",
    "vectorStores",
    "Reranking failed, using vector order",
    expect.objectContaining({ storeId: store.id, timedOut: false }),
  );
});

test("a reranker that never answers times out into vector order", async () => {
  service = makeService({ rerankTimeoutMs: 20 });
  const store = await rerankingStore();
  rerank.mockImplementation(
    (_query, _passages, signal) =>
      new Promise((_, reject) => {
        signal.addEventListener("abort", () => reject(signal.reason as Error), { once: true });
      }),
  );

  const result = await service.searchTimed(store.id, "q", { k: 3 });

  expect(result.rerank).toBe("fallback");
  expect(result.hits.map((entry) => entry.id)).toEqual(["a", "b", "c"]);
  expect(log).toHaveBeenCalledWith(
    "warn",
    "vectorStores",
    "Reranking failed, using vector order",
    expect.objectContaining({ timedOut: true }),
  );
});

test("a malformed answer is treated as a failure, not applied", async () => {
  const store = await rerankingStore();
  rerank.mockResolvedValue([1, 2]);
  const result = await service.searchTimed(store.id, "q", { k: 4 });
  expect(result.rerank).toBe("fallback");
  expect(result.hits.map((entry) => entry.id)).toEqual(["a", "b", "c", "d"]);
});

test("without a reranker attached the search still answers", async () => {
  service = makeService({ rerankers: undefined });
  const store = await rerankingStore();
  const result = await service.searchTimed(store.id, "q", { k: 1 });
  expect(result.rerank).toBe("fallback");
  expect(result.hits.map((entry) => entry.id)).toEqual(["a"]);
  service.attachRerankers(rerankers);
  rerank.mockResolvedValue([0, 0, 0, 9]);
  expect((await service.searchTimed(store.id, "q", { k: 1 })).hits[0]?.id).toBe("d");
});

test("cancelling the search is a cancellation, not a fallback", async () => {
  const store = await rerankingStore();
  const controller = new AbortController();
  rerank.mockImplementation(() => {
    controller.abort();
    return Promise.reject(new Error("aborted"));
  });
  await expect(service.searchTimed(store.id, "q", { k: 1 }, controller.signal)).rejects.toThrow();
  expect(log).not.toHaveBeenCalledWith(
    "warn",
    "vectorStores",
    "Reranking failed, using vector order",
    expect.anything(),
  );
});

test("the local reranker binds the store's model to the engine call", async () => {
  const engine = { rerank: vi.fn(() => Promise.resolve([1])) };
  const signal = AbortSignal.timeout(1_000);
  await localRerankers(engine)("curated:model:x").rerank("q", ["p"], signal);
  expect(engine.rerank).toHaveBeenCalledWith("curated:model:x", "q", ["p"], signal);
});
