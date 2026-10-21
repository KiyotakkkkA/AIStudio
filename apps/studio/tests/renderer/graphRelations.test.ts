import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  contract,
  GraphEdgeDto,
  GraphRelationTypeDto,
  VectorDocumentDto,
  VectorStoreDto,
  type Contract,
  type GraphNeighborDto,
  type RunCountsDto,
  type VectorSearchResultDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge";
import VectorStoreStore from "../../src/renderer/features/vector-stores/VectorStoreStore";
import GraphRelationStore from "../../src/renderer/features/vector-stores/GraphRelationStore";
import { mergeGraphNeighbors } from "../../src/renderer/features/vector-stores/graphExpansion";
import { EMBEDDER } from "./providerFixtures";

const STORE = "0199bb11-1111-7111-8111-0000000000d0";
const DOC_A = "0199bb11-1111-7111-8111-0000000000a1";
const DOC_B = "0199bb11-1111-7111-8111-0000000000a2";
const DOC_C = "0199bb11-1111-7111-8111-0000000000a3";
const LINKED = "01999a00-0004-7000-8000-000000000004";
const CITES = "01999a00-0001-7000-8000-000000000001";

const detail = VectorStoreDto.parse({
  id: STORE,
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
  documents: 3,
  vectors: 3,
  bytes: 1000,
  createdAt: 1,
  updatedAt: 1,
});
const relations = [
  GraphRelationTypeDto.parse({ id: CITES, name: "cites", description: "", directed: true }),
  GraphRelationTypeDto.parse({
    id: LINKED,
    name: "user_linked",
    description: "",
    directed: true,
  }),
];
const documents = [
  ["glossary.md", DOC_A],
  ["setup.md", DOC_B],
  ["faq.md", DOC_C],
].map(([path, id]) =>
  VectorDocumentDto.parse({
    id,
    storeId: STORE,
    sourcePath: path,
    contentHash: path,
    chunkCount: 4,
    bytes: 10,
    indexedAt: 1,
  }),
);
const edge = (from: string, to: string, relationTypeId = LINKED) =>
  GraphEdgeDto.parse({
    id: `0199bb11-1111-7111-8111-0000000000${from === DOC_A ? "e1" : "e2"}`,
    storeId: STORE,
    fromDocumentId: from,
    toDocumentId: to,
    relationTypeId,
    weight: null,
    metadata: null,
    createdAt: 1,
  });
const result: VectorSearchResultDto = {
  hits: [
    {
      id: "chunk-1",
      score: 0.9,
      documentId: DOC_A,
      chunkIndex: 0,
      path: "glossary.md",
      text: "Определение",
      section: "",
      page: null,
      payload: { chunkCount: 4 },
    },
  ],
  embeddingMs: 1,
  searchMs: 2,
};
const neighbor = (id: string, relation: string): GraphNeighborDto => ({
  node: { id, kind: "document" },
  edge: { from: DOC_A, to: id, relation, weight: null, metadata: null },
  direction: "outgoing",
});

const NO_RUNS: RunCountsDto = {
  total: 0,
  byStatus: {
    queued: 0,
    running: 0,
    blocked: 0,
    succeeded: 0,
    failed: 0,
    cancelled: 0,
    interrupted: 0,
  },
  byKind: {
    chat: 0,
    scenario: 0,
    agentic: 0,
    job: 0,
    indexing: 0,
    download: 0,
    browser: 0,
    skill: 0,
  },
};

function setup() {
  const bridge = createFakeBridge<Contract>();
  let edges: GraphEdgeDto[] = [];
  bridge.handle("providers.list", () => []);
  bridge.handle("vectorStores.list", () => [detail]);
  bridge.handle("vectorStores.get", () => detail);
  bridge.handle("vectorStores.sources.list", () => []);
  bridge.handle("vectorStores.documents.list", () => documents);
  bridge.handle("vectorStores.searchTimed", () => result);
  bridge.handle("runs.list", () => ({ items: [], counts: NO_RUNS }));
  bridge.handle("graphRelations.list", () => relations);
  bridge.handle("graphRelations.listEdges", () => edges);
  bridge.handle("graphRelations.addEdge", (input) => {
    const created = edge(input.fromDocumentId, input.toDocumentId, input.relationTypeId);
    edges = [...edges, created];
    return created;
  });
  bridge.handle("graphRelations.removeEdge", (input) => {
    const removed = edges.find((known) => known.fromDocumentId === input.fromDocumentId);
    edges = edges.filter((known) => known !== removed);
    return { id: removed!.id, removed: true } as const;
  });
  bridge.handle("graphRelations.neighbors", () => [neighbor(DOC_B, "user_linked")]);
  const store = new VectorStoreStore(createIpcClient(contract, bridge));
  return { bridge, store, graph: store.graph };
}

test("an edge needs two different documents and a relation type before it reaches the host", async () => {
  const { bridge, graph } = setup();
  await graph.load(detail.id);
  expect(graph.relationTypeId).toBe(LINKED);
  expect(graph.canAdd).toBe(false);
  graph.set("fromDocumentId", DOC_A);
  graph.set("toDocumentId", DOC_A);
  expect(graph.canAdd).toBe(false);
  expect(await graph.addEdge()).toBe(false);
  expect(bridge.calls.some((call) => call.channel === "graphRelations.addEdge")).toBe(false);
  expect(graph.error).toBeTruthy();
  graph.set("toDocumentId", DOC_B);
  expect(graph.canAdd).toBe(true);
  expect(await graph.addEdge()).toBe(true);
  expect(graph.edges).toHaveLength(1);
  expect(graph.error).toBeNull();
  // A drawn edge clears the picker so the next one starts from scratch.
  expect(graph.fromDocumentId).toBe("");
  expect(graph.toDocumentId).toBe("");
});

test("edges group under both ends and removing one drops it from the list", async () => {
  const { graph } = setup();
  await graph.load(detail.id);
  graph.set("fromDocumentId", DOC_A);
  graph.set("toDocumentId", DOC_B);
  await graph.addEdge();
  const groups = graph.groups(documents);
  expect(groups.map((group) => group.document.sourcePath)).toEqual(["glossary.md", "setup.md"]);
  expect(groups[0]?.outgoing).toHaveLength(1);
  expect(groups[0]?.incoming).toHaveLength(0);
  expect(groups[1]?.incoming).toHaveLength(1);
  expect(graph.relationName(LINKED)).toBe("Связано вручную");
  expect(await graph.removeEdge(groups[0]!.outgoing[0]!)).toBe(true);
  expect(graph.edges).toEqual([]);
  expect(graph.groups(documents)).toEqual([]);
});

test("a host that cannot answer leaves the tab empty rather than showing stale edges", async () => {
  const { bridge, graph } = setup();
  await graph.load(detail.id);
  graph.set("fromDocumentId", DOC_A);
  graph.set("toDocumentId", DOC_B);
  await graph.addEdge();
  bridge.fail("graphRelations.listEdges", AppErrorCode.NATIVE_ERROR, "Граф недоступен");
  await graph.load(detail.id);
  expect(graph.edges).toEqual([]);
  expect(graph.error).toBe("Граф недоступен");
  expect(graph.loaded).toBe(true);
});

test("expansion skips ranked and repeated documents, honours the cap and keeps the relation", () => {
  const hits = [{ documentId: DOC_A }];
  const neighborhoods = [
    {
      documentId: DOC_A,
      neighbors: [
        neighbor(DOC_A, "cites"),
        neighbor(DOC_B, "user_linked"),
        neighbor(DOC_B, "cites"),
        { ...neighbor("chunk-9", "cites"), node: { id: "chunk-9", kind: "chunk" as const } },
        neighbor(DOC_C, "cites"),
      ],
    },
  ];
  expect(mergeGraphNeighbors(hits, neighborhoods, documents, 5)).toEqual([
    {
      documentId: DOC_B,
      relation: "user_linked",
      direction: "outgoing",
      viaDocumentId: DOC_A,
      path: "setup.md",
      chunkCount: 4,
    },
    {
      documentId: DOC_C,
      relation: "cites",
      direction: "outgoing",
      viaDocumentId: DOC_A,
      path: "faq.md",
      chunkCount: 4,
    },
  ]);
  expect(mergeGraphNeighbors(hits, neighborhoods, documents, 1)).toHaveLength(1);
  expect(mergeGraphNeighbors(hits, neighborhoods, documents, 0)).toEqual([]);
  // A document the store does not know about is still offered, under its raw identifier.
  expect(mergeGraphNeighbors(hits, neighborhoods, [], 1)[0]).toMatchObject({
    path: DOC_B,
    chunkCount: null,
  });
});

test("the toggle decides whether a search is widened, and graph rows carry a relation, not a score", async () => {
  const { bridge, store, graph } = setup();
  await store.load();
  store.set("query", "определение");
  await store.search();
  expect(store.graphRows).toEqual([]);
  expect(bridge.calls.some((call) => call.channel === "graphRelations.neighbors")).toBe(false);

  graph.setExpandEnabled(true);
  await store.search();
  expect(store.rows).toHaveLength(1);
  expect(store.graphRows).toHaveLength(1);
  const row = store.graphRows[0]!;
  expect(row.relation).toBe("user_linked");
  expect(row.path).toBe("setup.md");
  expect(row.via).toBe("glossary.md");
  expect(row.position).toBe("4 чанков");
  expect(row).not.toHaveProperty("score");

  // The cap is honoured from the field, and a new search starts without the previous expansion.
  graph.set("expandMax", "0");
  await store.search();
  expect(store.graphRows).toEqual([]);
});

test("a graph that fails to answer leaves the vector results untouched", async () => {
  const { bridge, store, graph } = setup();
  await store.load();
  graph.setExpandEnabled(true);
  bridge.fail("graphRelations.neighbors", AppErrorCode.NATIVE_ERROR, "Граф недоступен");
  store.set("query", "определение");
  await store.search();
  expect(store.rows).toHaveLength(1);
  expect(store.graphRows).toEqual([]);
  expect(store.error).toBeNull();
  expect(graph.error).toBe("Граф недоступен");
});

test("relation types are read once and the store forgets edges when the selection changes", async () => {
  const bridge = createFakeBridge<Contract>();
  bridge.handle("graphRelations.list", () => relations);
  bridge.handle("graphRelations.listEdges", () => [edge(DOC_A, DOC_B)]);
  const graph = new GraphRelationStore(createIpcClient(contract, bridge));
  await graph.load(detail.id);
  await graph.load(detail.id);
  expect(bridge.calls.filter((call) => call.channel === "graphRelations.list")).toHaveLength(1);
  expect(graph.edges).toHaveLength(1);
  graph.reset();
  expect(graph.edges).toEqual([]);
  expect(graph.storeId).toBeNull();
  expect(graph.relations).toHaveLength(2);
});
