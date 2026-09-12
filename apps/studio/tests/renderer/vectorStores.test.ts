import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  contract,
  VectorStoreDto,
  type Contract,
  type VectorSearchResultDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge";
import VectorStoreStore from "../../src/renderer/features/vector-stores/VectorStoreStore";
import VectorStoreFormVm from "../../src/renderer/features/vector-stores/VectorStoreFormVm";
import { searchRows } from "../../src/renderer/features/vector-stores/searchRows";
import { EMBEDDER, summary } from "./providerFixtures";

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
  expect(vm.toUpdateInput()).toEqual({ id: detail.id, name: "Renamed", description: "Manuals" });
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
    expect(input).toEqual({ id: detail.id, name: "Renamed", description: "Manuals" });
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
