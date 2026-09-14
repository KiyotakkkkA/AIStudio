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
  chunkPlan,
  ocrCandidates,
  rerankCandidates,
} from "../../src/renderer/features/vector-stores/autofill";
import { EMBEDDER, MODEL, model, provider as providerDetail, summary } from "./providerFixtures";

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
      payload: { text: "Hello", chunkCount: 5 },
    },
    { id: "b", score: 0.1, documentId: "doc", chunkIndex: 0, path: "manual.md", payload: null },
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
    });
    return { ...detail, ...input };
  });
  expect(await store.save()).toBe(true);
});

test("payload mapping does not invent text or chunk totals", () => {
  const hit = result.hits[0]!;
  expect(searchRows([{ ...hit, payload: "plain" }], 0)[0]).toMatchObject({
    text: "plain",
    position: "2",
  });
  expect(searchRows([{ ...hit, payload: { chunkCount: 1 } }], 0)[0]?.text).toContain("отсутствует");
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
