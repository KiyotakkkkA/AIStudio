import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  contract,
  VectorStoreDto,
  type CatalogueItemDto,
  type Contract,
  type DeviceProfileDto,
  type VectorSearchResultDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge";
import VectorStoreStore from "../../src/renderer/features/vector-stores/VectorStoreStore";
import VectorStoreFormVm from "../../src/renderer/features/vector-stores/VectorStoreFormVm";
import { searchRows } from "../../src/renderer/features/vector-stores/searchRows";
import {
  formatDuration,
  formatThroughput,
  PRIMARY_ACTION_LABELS,
  primaryAction,
  splitPath,
  storeFacts,
} from "../../src/renderer/features/vector-stores/vectorPresentation";
import {
  chunkPlan,
  embeddingPrefixesFor,
  ocrCandidates,
  rerankCandidates,
} from "../../src/renderer/features/vector-stores/autofill";
import {
  EMBEDDER,
  MODEL,
  OLLAMA,
  model,
  provider as providerDetail,
  summary,
} from "./providerFixtures";

const provider = summary({ id: EMBEDDER, capabilities: ["embedding"] });
const detail = VectorStoreDto.parse({
  id: "0199bb11-1111-7111-8111-000000000099",
  name: "Knowledge",
  description: "Manuals",
  backend: "lancedb",
  embeddingProviderId: EMBEDDER,
  embeddingModelId: "embed",
  dimension: 3,
  metric: "cosine",
  chunkSize: 256,
  chunkOverlap: 32,
  indexType: "FLAT",
  status: "healthy",
  lastIndexedAt: null,
  documents: 2,
  vectors: 3,
  bytes: 1000,
  createdAt: 1,
  updatedAt: 1,
});
const result: VectorSearchResultDto = {
  hits: [
    {
      id: "a",
      score: 0.9,
      documentId: "doc",
      chunkIndex: 1,
      path: "manual.md",
      text: "Hello",
      section: "",
      page: null,
      payload: { chunkCount: 5 },
    },
    {
      id: "b",
      score: 0.1,
      documentId: "doc",
      chunkIndex: 0,
      path: "manual.md",
      text: "",
      section: "",
      page: null,
      payload: null,
    },
  ],
  embeddingMs: 18,
  searchMs: 45,
};

function setup() {
  const bridge = createFakeBridge<Contract>();
  bridge.handle("providers.list", (input) => {
    expect(input.capability).toBe("embedding");
    return [provider];
  });
  bridge.handle("providers.get", ({ id }) => ({
    ...providerDetail({ ...provider, id }),
    models: [model({ id: MODEL, providerId: id, externalId: "bge-m3:latest" })],
  }));
  bridge.handle("vectorStores.list", () => [{ ...detail, vectors: 0 }]);
  bridge.handle("vectorStores.get", ({ id }) => ({ ...detail, id }));
  bridge.handle("vectorStores.reconcile", () => ({ ...detail, status: "broken" }));
  bridge.handle("vectorStores.searchTimed", () => result);
  bridge.handle("vectorStores.remove", ({ id }) => ({ id, removed: true }));
  return { bridge, store: new VectorStoreStore(createIpcClient(contract, bridge)) };
}

test("form requires explicit dimension, enabled embedding provider and valid chunk settings", () => {
  const vm = new VectorStoreFormVm(null, [provider]);
  vm.set("name", " New ");
  vm.set("embeddingProviderId", EMBEDDER);
  vm.set("embeddingModelId", "embed");
  expect(vm.validate()).toBe(false);
  expect(vm.errors.dimension).toBeTruthy();
  for (const dimension of ["0", "1.5", "NaN", "65537"]) {
    vm.set("dimension", dimension);
    expect(vm.validate()).toBe(false);
  }
  vm.set("dimension", "3");
  vm.set("chunkOverlap", "256");
  expect(vm.validate()).toBe(false);
  vm.set("chunkOverlap", "0");
  expect(vm.validate()).toBe(true);
  expect(vm.toCreateInput().dimension).toBe(3);
  expect(vm.toCreateInput().name).toBe("New");
  vm.set("embeddingProviderId", "");
  expect(vm.validate()).toBe(false);
  const disabled = new VectorStoreFormVm(null, [{ ...provider, enabled: false }]);
  disabled.set("name", "New");
  disabled.set("embeddingProviderId", EMBEDDER);
  disabled.set("embeddingModelId", "embed");
  disabled.set("dimension", "3");
  expect(disabled.validate()).toBe(false);
});

test("editing preserves immutable configuration and indexed chunk settings", () => {
  const vm = new VectorStoreFormVm(detail, [provider]);
  expect(vm.chunkLocked).toBe(true);
  vm.set("name", "Renamed");
  expect(vm.toUpdateInput()).toEqual({
    id: detail.id,
    name: "Renamed",
    description: "Manuals",
    rerank: { enabled: false, modelRef: "", candidates: 50 },
    ocr: { enabled: false, modelRef: "", language: "auto", minCharsPerPage: 200 },
    searchMode: "hybrid",
    chatMinScore: 0.3,
  });
});

test("load uses reconciled detail; search maps timings, positions and below-floor results", async () => {
  const { store, bridge } = setup();
  await store.load();
  expect(store.detail?.vectors).toBe(3);
  expect(store.stores[0]?.vectors).toBe(3);
  store.set("query", "hello");
  await store.search();
  expect(store.result?.embeddingMs).toBe(18);
  expect(store.hitCount).toBe(1);
  expect(store.rows[0]).toMatchObject({
    score: "0.90",
    width: "90%",
    path: "manual.md",
    position: "2/5",
    text: "Hello",
    belowFloor: false,
  });
  expect(store.rows[1]?.belowFloor).toBe(true);
  expect(bridge.calls.find((c) => c.channel === "vectorStores.searchTimed")?.payload).toEqual({
    storeId: detail.id,
    query: "hello",
    k: 5,
    minScore: 0,
    mode: "vector",
  });
  await store.reconcile();
  expect(store.detail?.status).toBe("broken");
  await store.remove();
  expect(store.detail).toBeNull();
  expect(store.stores).toEqual([]);
});

test("invalid search never calls bridge and empty results stay distinct from failure", async () => {
  const { store, bridge } = setup();
  await store.load();
  await store.search();
  expect(bridge.calls.some((c) => c.channel === "vectorStores.searchTimed")).toBe(false);
  bridge.handle("vectorStores.searchTimed", () => ({ hits: [], embeddingMs: 1, searchMs: 2 }));
  store.set("query", "missing");
  await store.search();
  expect(store.hitCount).toBe(0);
  expect(store.result).not.toBeNull();
  expect(store.error).toBeNull();
  bridge.fail("vectorStores.searchTimed", AppErrorCode.PROVIDER_UNREACHABLE, "Offline");
  await store.search();
  expect(store.result).toBeNull();
  expect(store.error).toBeTruthy();
  expect(store.searching).toBe(false);
});

test("late search cannot replace another store's results", async () => {
  const { store, bridge } = setup();
  await store.load();
  let resolve!: (value: VectorSearchResultDto) => void;
  bridge.handle(
    "vectorStores.searchTimed",
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  store.set("query", "hello");
  const pending = store.search();
  await store.select(
    VectorStoreDto.parse({ ...detail, id: "0199bb11-1111-7111-8111-000000000098" }).id,
  );
  resolve(result);
  await pending;
  expect(store.result).toBeNull();
  expect(store.searching).toBe(false);
});

test("create and edit use real contract inputs; failures preserve the form", async () => {
  const { store, bridge } = setup();
  await store.load();
  store.create();
  const vm = store.form!;
  vm.set("name", "New");
  vm.set("embeddingProviderId", EMBEDDER);
  vm.set("embeddingModelId", "embed");
  vm.set("dimension", "3");
  bridge.fail("vectorStores.create", AppErrorCode.CONFLICT, "Duplicate");
  expect(await store.save()).toBe(false);
  expect(store.form).toBe(vm);
  bridge.handle("vectorStores.create", (input) => ({ ...detail, ...input }));
  expect(await store.save()).toBe(true);
  expect(store.form).toBeNull();
  store.edit();
  store.form!.set("name", "Renamed");
  bridge.handle("vectorStores.update", (input) => {
    expect(input).toEqual({
      id: detail.id,
      name: "Renamed",
      description: "Manuals",
      rerank: { enabled: false, modelRef: "", candidates: 50 },
      ocr: { enabled: false, modelRef: "", language: "auto", minCharsPerPage: 200 },
      searchMode: "hybrid",
      chatMinScore: 0.3,
    });
    return { ...detail, ...input };
  });
  expect(await store.save()).toBe(true);
});

test("payload mapping does not invent text or chunk totals", () => {
  const hit = result.hits[0]!;
  expect(searchRows([{ ...hit, text: "", payload: "plain" }], 0)[0]).toMatchObject({
    text: "plain",
    position: "2",
  });
  expect(searchRows([{ ...hit, text: "", payload: { text: "v1" } }], 0)[0]?.text).toBe("v1");
  expect(searchRows([{ ...hit, text: "", payload: { chunkCount: 1 } }], 0)[0]?.text).toContain(
    "отсутствует",
  );
});

const RERANKER: CatalogueItemDto = {
  ref: "curated:model:bge-reranker-v2-m3-f16",
  kind: "model",
  source: "curated",
  name: "bge-reranker-v2-m3",
  displayName: "BGE Reranker v2 M3",
  description: "",
  sizeBytes: 1,
  url: "https://example.invalid/reranker",
  tags: ["переранжирование"],
  state: "installed",
  downloadable: false,
};
const OCR_MODEL: CatalogueItemDto = {
  ...RERANKER,
  ref: "curated:model:qwen2.5-vl-7b",
  name: "qwen2.5-vl:7b",
  displayName: "Qwen2.5-VL 7B Instruct (OCR)",
  tags: ["OCR", "зрение"],
};
const EMBEDDING_ITEM: CatalogueItemDto = {
  ...RERANKER,
  ref: "curated:embedding:bge-m3-f16",
  kind: "embedding",
  name: "bge-m3",
  displayName: "BGE-M3",
  dimension: 1024,
  tags: ["многоязычная"],
};
const DEVICE: DeviceProfileDto = {
  platform: "win32",
  arch: "x64",
  cpuModel: "Test CPU",
  cpuCores: 8,
  totalMemoryBytes: 16 * 1024 ** 3,
  freeMemoryBytes: 4 * 1024 ** 3,
  freeDiskBytes: 500 * 1024 ** 3,
  gpu: null,
  tier: "medium",
  measuredAt: 1 as DeviceProfileDto["measuredAt"],
};
const CATALOGUE = [RERANKER, OCR_MODEL, EMBEDDING_ITEM];
const MODELS = new Map([
  [EMBEDDER as string, [model({ id: MODEL, providerId: EMBEDDER, externalId: "bge-m3:latest" })]],
]);

test("a store saved before the advanced settings existed reads with both stages off", () => {
  expect(detail.rerank).toEqual({ enabled: false, modelRef: "", candidates: 50 });
  expect(detail.ocr.language).toBe("auto");
});

test("auto-fill takes the model it can see on disk and leaves the name and description alone", () => {
  const vm = new VectorStoreFormVm(null, [provider], MODELS, {
    catalogue: CATALOGUE,
    device: DEVICE,
  });
  vm.set("name", "Kept");
  vm.set("description", "Also kept");
  vm.autofill();
  expect(vm.name).toBe("Kept");
  expect(vm.description).toBe("Also kept");
  expect(vm.embeddingProviderId).toBe(EMBEDDER);
  // The downloaded artefact names the provider model and settles the dimension.
  expect(vm.embeddingModelId).toBe("bge-m3:latest");
  expect(vm.dimension).toBe("1024");
  expect(vm.chunkSize).toBe("512");
  expect(vm.chunkOverlap).toBe("64");
  expect(vm.advancedOpen).toBe(true);
  expect(vm.rerankEnabled).toBe(true);
  expect(vm.rerankModelRef).toBe(RERANKER.ref);
  expect(vm.rerankCandidatesCount).toBe("50");
  // Sixteen gigabytes and no discrete GPU is not enough to run a vision model per page.
  expect(vm.ocrEnabled).toBe(false);
  expect(vm.autofillNote).toContain("BGE Reranker");
  expect(vm.validate()).toBe(true);
});

test("auto-fill is cautious without a device profile and enables OCR only with the hardware for it", () => {
  const blind = new VectorStoreFormVm(null, [provider], MODELS, { catalogue: CATALOGUE });
  blind.autofill();
  expect(blind.chunkSize).toBe("256");
  expect(blind.rerankEnabled).toBe(false);
  expect(blind.ocrEnabled).toBe(false);

  const strong = new VectorStoreFormVm(null, [provider], MODELS, {
    catalogue: CATALOGUE,
    device: {
      ...DEVICE,
      tier: "high",
      gpu: { vendor: "NVIDIA", model: "RTX", vramBytes: null, discrete: true },
    },
  });
  strong.autofill();
  expect(strong.chunkSize).toBe("768");
  expect(strong.rerankCandidatesCount).toBe("80");
  expect(strong.ocrEnabled).toBe(true);
  expect(strong.ocrModelRef).toBe(OCR_MODEL.ref);
  expect(strong.toCreateInput).toBeTruthy();
});

test("auto-fill on an existing store touches only what is still editable", () => {
  const vm = new VectorStoreFormVm(detail, [provider], MODELS, {
    catalogue: CATALOGUE,
    device: DEVICE,
  });
  expect(vm.chunkLocked).toBe(true);
  vm.autofill();
  expect(vm.embeddingModelId).toBe(detail.embeddingModelId);
  expect(vm.dimension).toBe(String(detail.dimension));
  expect(vm.chunkSize).toBe(String(detail.chunkSize));
  expect(vm.rerankEnabled).toBe(true);
  expect(vm.toUpdateInput()).toMatchObject({ rerank: { modelRef: RERANKER.ref } });
});

test("an enabled stage needs an installed model, and switching one on picks the only one there is", () => {
  const vm = new VectorStoreFormVm(null, [provider], MODELS, { catalogue: CATALOGUE });
  vm.set("name", "New");
  vm.set("embeddingProviderId", EMBEDDER);
  vm.set("embeddingModelId", "bge-m3:latest");
  vm.set("dimension", "1024");
  vm.setRerankEnabled(true);
  expect(vm.rerankModelRef).toBe(RERANKER.ref);
  vm.set("rerankModelRef", "");
  expect(vm.validate()).toBe(false);
  expect(vm.errors.rerank).toBeTruthy();
  vm.setRerankEnabled(false);
  vm.setOcrEnabled(true);
  vm.set("ocrModelRef", "");
  expect(vm.validate()).toBe(false);
  expect(vm.errors.ocr).toBeTruthy();
  vm.setOcrEnabled(false);
  expect(vm.validate()).toBe(true);
  expect(vm.toCreateInput().rerank.enabled).toBe(false);
});

test("the catalogue offers only installed rerankers and OCR models, never each other", () => {
  const queued: CatalogueItemDto = { ...RERANKER, ref: "curated:model:other", state: "queued" };
  expect(rerankCandidates([...CATALOGUE, queued]).map((item) => item.ref)).toEqual([RERANKER.ref]);
  expect(ocrCandidates(CATALOGUE).map((item) => item.ref)).toEqual([OCR_MODEL.ref]);
  expect(chunkPlan("low")).toEqual({ chunkSize: 256, chunkOverlap: 32 });
});

test("a missing downloads catalogue or device profile leaves the page usable", async () => {
  const { store } = setup();
  await store.load();
  expect(store.catalogue).toEqual([]);
  expect(store.device).toBeNull();
  store.create();
  store.form!.autofill();
  expect(store.form!.autofillNote).toContain("недоступен");
});

test("an estimate is coarsened as it grows, and reads as a dash when there is nothing to go on", () => {
  expect(formatDuration(undefined)).toBe("—");
  expect(formatDuration(Number.POSITIVE_INFINITY)).toBe("—");
  expect(formatDuration(400)).toBe("1 с");
  expect(formatDuration(45_000)).toBe("45 с");
  expect(formatDuration(90_000)).toBe("2 мин");
  expect(formatDuration(3_600_000)).toBe("1 ч");
  expect(formatDuration(5_400_000)).toBe("1 ч 30 мин");
});

test("throughput is rounded to something readable and never shows a rate of zero", () => {
  expect(formatThroughput(0)).toBe("— фрагм./с");
  expect(formatThroughput(2.34)).toBe("2,3 фрагм./с");
  expect(formatThroughput(148.6)).toBe("149 фрагм./с");
});

test("documents appear as the run reports them, newest first and never twice", () => {
  const store = new VectorStoreStore(createIpcClient<Contract>(contract, createFakeBridge()));
  const FIRST = "0199bb11-1111-7111-8111-0000000000a1";
  const SECOND = "0199bb11-1111-7111-8111-0000000000a2";
  const document = (id: string, path: string, chunks: number) => ({
    id,
    storeId: detail.id,
    sourcePath: path,
    contentHash: "h",
    chunkCount: chunks,
    bytes: 10,
    indexedAt: 1,
  });

  store.receiveDocument(document(FIRST, "first.md", 3));
  store.receiveDocument(document(SECOND, "second.md", 5));
  expect(store.documents.map((row) => row.id)).toEqual([SECOND, FIRST]);

  // A re-indexed file moves to the front rather than being listed a second time.
  store.receiveDocument(document(FIRST, "first.md", 9));
  expect(store.documents.map((row) => row.id)).toEqual([FIRST, SECOND]);
  expect(store.documents[0]?.chunkCount).toBe(9);

  store.receiveDocument({ nonsense: true });
  expect(store.documents).toHaveLength(2);
});

const LOCAL = "0199bb11-1111-7111-8111-000000000003" as typeof EMBEDDER;
const localFile = (index: number, externalId: string) =>
  model({
    id: `0199bb11-2222-7111-8111-00000000010${String(index)}` as typeof MODEL,
    providerId: LOCAL,
    externalId,
  });
// What the local provider lists for a machine with the curated downloads: every weight file,
// sorted the way the models table sorts them — uppercase first.
const LOCAL_FILES = [
  localFile(1, "Qwen2.5-VL-7B-Instruct-Q4_K_M.gguf"),
  localFile(2, "bge-m3-FP16.gguf"),
  localFile(3, "bge-reranker-v2-m3-FP16.gguf"),
  localFile(4, "mmproj-Qwen2.5-VL-7B-Instruct-Q8_0.gguf"),
];
const ON_DISK: CatalogueItemDto = {
  ...EMBEDDING_ITEM,
  url: "https://huggingface.co/gpustack/bge-m3-GGUF/resolve/main/bge-m3-FP16.gguf",
};

test("auto-fill recognises a downloaded embedding by the file the local provider lists", () => {
  const chat = summary({ id: OLLAMA, capabilities: ["text"] });
  const local = summary({ id: LOCAL, capabilities: ["embedding"] });
  const vm = new VectorStoreFormVm(null, [chat, local], new Map([[LOCAL as string, LOCAL_FILES]]), {
    catalogue: [RERANKER, OCR_MODEL, ON_DISK],
    device: DEVICE,
  });
  vm.autofill();
  // A provider that cannot embed is never offered, even when it comes first.
  expect(vm.embeddingProviderId).toBe(LOCAL);
  expect(vm.embeddingModelId).toBe("bge-m3-FP16.gguf");
  expect(vm.dimension).toBe("1024");
});

test("auto-fill without a recognised download still skips vision models, projectors and rerankers", () => {
  const local = summary({ id: LOCAL, capabilities: ["embedding"] });
  const models = new Map([[LOCAL as string, LOCAL_FILES]]);
  const vm = new VectorStoreFormVm(null, [local], models, { device: DEVICE });
  vm.autofill();
  expect(vm.embeddingModelId).toBe("bge-m3-FP16.gguf");

  // A vision model left in the form by an earlier pick is not kept either.
  const stale = new VectorStoreFormVm(null, [local], models, { device: DEVICE });
  stale.set("embeddingProviderId", LOCAL);
  stale.set("embeddingModelId", "Qwen2.5-VL-7B-Instruct-Q4_K_M.gguf");
  stale.autofill();
  expect(stale.embeddingModelId).toBe("bge-m3-FP16.gguf");

  const onlyVision = new VectorStoreFormVm(
    null,
    [local],
    new Map([[LOCAL as string, [LOCAL_FILES[0]!, LOCAL_FILES[3]!]]]),
    { device: DEVICE },
  );
  onlyVision.autofill();
  expect(onlyVision.embeddingModelId).toBe("");
});

test("a v2 store searches in its own mode, shows provenance and loads neighbours on demand", async () => {
  const { store, bridge } = setup();
  const v2 = { ...detail, schemaVersion: 2, searchMode: "hybrid" as const };
  bridge.handle("vectorStores.get", ({ id }) => ({ ...v2, id }));
  bridge.handle("vectorStores.searchTimed", () => ({
    ...result,
    mode: "hybrid" as const,
    hits: [
      {
        ...result.hits[0]!,
        page: 4,
        section: "Отпуск › Порядок",
        provenance: "keyword" as const,
        keywordScore: 3.25,
        score: 0.2,
      },
    ],
  }));
  bridge.handle("vectorStores.chunks", (input) => {
    expect(input).toEqual({ storeId: detail.id, documentId: "doc", indices: [0, 2] });
    return [
      {
        id: "doc:0",
        payload: {},
        documentId: "doc",
        chunkIndex: 0,
        path: "manual.md",
        text: "Before",
        section: "",
        page: 3,
      },
      {
        id: "doc:2",
        payload: {},
        documentId: "doc",
        chunkIndex: 2,
        path: "manual.md",
        text: "After",
        section: "",
        page: 4,
      },
    ];
  });
  await store.load();
  expect(store.hybridAvailable).toBe(true);
  store.set("query", "КНД-1151020");
  await store.search();
  expect(bridge.calls.find((c) => c.channel === "vectorStores.searchTimed")?.payload).toMatchObject(
    {
      mode: "hybrid",
    },
  );
  expect(store.rows[0]).toMatchObject({
    provenance: "ключевые слова",
    keywordScore: "3.25",
    place: "стр. 4 · Отпуск › Порядок",
    belowFloor: false,
  });

  await store.toggleNeighbours("a");
  expect(store.neighbourRowsFor("a")).toEqual([
    { id: "doc:0", before: true, label: "Предыдущий фрагмент 1", place: "стр. 3", text: "Before" },
    { id: "doc:2", before: false, label: "Следующий фрагмент 3", place: "стр. 4", text: "After" },
  ]);
  await store.toggleNeighbours("a");
  expect(store.neighbourRowsFor("a")).toBeNull();

  store.setSearchMode("vector");
  await store.search();
  expect(
    bridge.calls.filter((c) => c.channel === "vectorStores.searchTimed").at(-1)?.payload,
  ).toMatchObject({
    mode: "vector",
  });
});

test("a model that needs prefixes fills them, from the catalogue first", () => {
  const vm = new VectorStoreFormVm(null, [provider]);
  vm.set("embeddingModelId", "multilingual-e5-large");
  expect([vm.queryPrefix, vm.documentPrefix]).toEqual(["query: ", "passage: "]);
  vm.set("embeddingModelId", "bge-m3:latest");
  expect([vm.queryPrefix, vm.documentPrefix]).toEqual(["", ""]);
  expect(embeddingPrefixesFor("nomic-embed-text")).toEqual({
    queryPrefix: "search_query: ",
    documentPrefix: "search_document: ",
  });
  expect(
    embeddingPrefixesFor("custom", { ...RERANKER, name: "custom", queryPrefix: "Q: " }),
  ).toEqual({ queryPrefix: "Q: ", documentPrefix: "" });
  const locked = new VectorStoreFormVm({ ...detail, queryPrefix: "query: " }, [provider]);
  locked.set("embeddingModelId", "nomic-embed-text");
  expect(locked.queryPrefix).toBe("query: ");
});

test("test search marks rows under the chat floor, which the Settings tab edits", async () => {
  const { store, bridge } = setup();
  bridge.handle("vectorStores.get", ({ id }) => ({ ...detail, id, chatMinScore: 0.95 }));
  await store.load();
  expect(store.minScore).toBe("0.95");
  expect(store.chatMinScore).toBe("0.95");
  store.set("query", "hello");
  await store.search();
  expect(store.rows.map((row) => row.belowFloor)).toEqual([true, true]);

  expect(store.chatMinScoreChanged).toBe(false);
  store.set("chatMinScore", "1.5");
  expect(await store.saveChatMinScore()).toBe(false);
  expect(store.error).toContain("от 0 до 1");
  store.set("chatMinScore", "0,5");
  bridge.handle("vectorStores.update", (input) => {
    expect(input).toEqual({ id: detail.id, chatMinScore: 0.5 });
    return { ...detail, chatMinScore: 0.5 };
  });
  expect(await store.saveChatMinScore()).toBe(true);
  expect(store.detail?.chatMinScore).toBe(0.5);
  expect(store.minScore).toBe("0.5");
  expect(store.rows.map((row) => row.belowFloor)).toEqual([false, true]);
});

test("the header offers the one action the store needs next", () => {
  const base = {
    status: "healthy",
    indexing: false,
    sources: 1,
    outdated: false,
    vectors: 3,
  } as const;
  expect(primaryAction(base)).toBe("update");
  expect(primaryAction({ ...base, indexing: true, status: "broken" })).toBe("stop");
  expect(primaryAction({ ...base, status: "broken" })).toBe("reconcile");
  expect(primaryAction({ ...base, status: "stale" })).toBe("reconcile");
  expect(primaryAction({ ...base, sources: 0 })).toBe("add");
  expect(primaryAction({ ...base, outdated: true })).toBe("rebuild");
  expect(primaryAction({ ...base, outdated: true, vectors: 0 })).toBe("update");
  expect(PRIMARY_ACTION_LABELS[primaryAction({ ...base, sources: 0 })]).toBe(
    PRIMARY_ACTION_LABELS.add,
  );
});

test("store facts count in words and say when the store was never indexed", () => {
  expect(storeFacts({ documents: 1, vectors: 22, bytes: 2048, lastIndexedAt: null })).toEqual([
    "1 документ",
    "22 фрагмента",
    "2.0 KB",
    "ещё не индексировалось",
  ]);
  expect(storeFacts({ documents: 5, vectors: 11, bytes: 0, lastIndexedAt: 0 })[3]).toMatch(
    /^обновлено /,
  );
});

test("a document path splits into its file name and folder on either separator", () => {
  expect(splitPath("C:\\docs\\handbook\\intro.md")).toEqual({
    name: "intro.md",
    folder: "C:\\docs\\handbook",
  });
  expect(splitPath("faq/general.md#L96")).toEqual({ name: "general.md#L96", folder: "faq" });
  expect(splitPath("readme.md")).toEqual({ name: "readme.md", folder: "" });
});

test("duplicating opens a create form with the configuration and none of the data", async () => {
  const { store, bridge } = setup();
  await store.load();
  store.duplicate();
  const vm = store.form!;
  expect(vm.isNew).toBe(true);
  expect(vm.chunkLocked).toBe(false);
  expect(vm.name).toBe("Knowledge (копия)");
  expect(vm.embeddingModelId).toBe("embed");
  expect(vm.dimension).toBe("3");
  bridge.handle("vectorStores.create", (input) =>
    VectorStoreDto.parse({
      ...detail,
      ...input,
      id: "0199bb11-1111-7111-8111-000000000097",
      documents: 0,
      vectors: 0,
    }),
  );
  bridge.handle("vectorStores.sources.list", () => []);
  bridge.handle("vectorStores.documents.list", () => []);
  expect(await store.save()).toBe(true);
  const call = bridge.calls.find((entry) => entry.channel === "vectorStores.create");
  expect(call?.payload).toMatchObject({
    name: "Knowledge (копия)",
    embeddingProviderId: EMBEDDER,
    embeddingModelId: "embed",
    dimension: 3,
    chunkSize: 256,
    chunkOverlap: 32,
  });
  expect(store.detail?.id).toBe("0199bb11-1111-7111-8111-000000000097");
  expect(store.sources).toEqual([]);
  expect(store.documents).toEqual([]);
});
