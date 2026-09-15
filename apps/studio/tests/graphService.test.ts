import { afterEach, beforeEach, expect, test, vi } from "vitest";
import {
  AppErrorCode,
  contract,
  createBrandedId,
  DocumentId,
  GraphRelationTypeId,
  VectorStoreId,
  type Json,
} from "@zvs/shared";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { FakeGraphCore } from "../../../test/helpers/FakeGraphCore.ts";
import { createId } from "../src/host/platform/ids.ts";
import { GraphService } from "../src/host/services/GraphService.ts";
import { createGraphRelationHandlers } from "../src/host/ipc/graphRelations.ts";

let db: TemporaryDatabase;
let core: FakeGraphCore;
let service: GraphService;
let storeId: VectorStoreId;
let otherStoreId: VectorStoreId;
let documents: DocumentId[];
let foreign: DocumentId;
const log = vi.fn();

function relation(name: string): GraphRelationTypeId {
  const row = db.client.repositories.graphRelations.findByName(name);
  if (!row) throw new Error(`missing relation ${name}`);
  return createBrandedId(GraphRelationTypeId, row.id);
}

function createStore(name: string, providerId: string): VectorStoreId {
  const row = db.client.repositories.vectorStores.create({
    id: createId(),
    name,
    embeddingProviderId: providerId,
    embeddingModelId: "embed-model",
    dimension: 3,
    metric: "cosine",
    chunkSize: 256,
    chunkOverlap: 32,
    createdAt: 1,
    updatedAt: 1,
  });
  return createBrandedId(VectorStoreId, row.id);
}

function createDocument(store: VectorStoreId, sourcePath: string): DocumentId {
  const row = db.client.repositories.vectorDocuments.recordIndexed({
    id: createId(),
    storeId: store,
    sourcePath,
    contentHash: sourcePath,
    chunkCount: 1,
    bytes: 10,
    indexedAt: 1,
  });
  return createBrandedId(DocumentId, row.id);
}

beforeEach(() => {
  db = temporaryDatabase();
  core = new FakeGraphCore();
  const provider = db.client.repositories.providers.create({
    name: "Embedding",
    kind: "ollama",
    baseUrl: "http://localhost:11434",
    capabilities: ["embedding"],
    createdAt: 1,
    updatedAt: 1,
  });
  storeId = createStore("Knowledge", provider.id);
  otherStoreId = createStore("Archive", provider.id);
  documents = ["a.md", "b.md", "c.md"].map((path) => createDocument(storeId, path));
  foreign = createDocument(otherStoreId, "a.md");
  service = new GraphService({
    data: db.client,
    core,
    logger: { log, close() {} },
    clock: () => 1000,
  });
  vi.clearAllMocks();
});
afterEach(() => db.dispose());

function link(from: number, to: number, name = "cites", metadata: Json = null) {
  return service.addEdge({
    storeId,
    fromDocumentId: documents[from]!,
    toDocumentId: documents[to]!,
    relationTypeId: relation(name),
    weight: null,
    metadata,
  });
}

test("the seeded catalog carries the four starter relations", () => {
  const relations = service.listRelations();
  expect(relations.map((row) => row.name)).toEqual([
    "cites",
    "co_occurs",
    "derived_from",
    "user_linked",
  ]);
  expect(relations.find((row) => row.name === "co_occurs")?.directed).toBe(false);
  expect(relations.find((row) => row.name === "cites")?.directed).toBe(true);
  expect(contract["graphRelations.list"].output.parse(relations)).toEqual(relations);
});

test("adding an edge writes the row and the engine half, metadata intact", async () => {
  const edge = await link(0, 1, "user_linked", { note: "вручную" });
  expect(db.client.repositories.graphEdges.listByStore(storeId)).toHaveLength(1);
  expect(core.edgesIn(storeId)).toEqual([
    {
      from: documents[0],
      to: documents[1],
      relation: "user_linked",
      weight: null,
      metadata: { note: "вручную" },
    },
  ]);
  expect(contract["graphRelations.addEdge"].output.parse(edge)).toEqual(edge);
});

test("a failing engine rolls the row back instead of leaving a half-written edge", async () => {
  vi.spyOn(core.graph, "addEdge").mockRejectedValueOnce(new Error("engine down"));
  await expect(link(0, 1)).rejects.toThrow("engine down");
  expect(db.client.repositories.graphEdges.listByStore(storeId)).toEqual([]);
  expect(log).toHaveBeenCalled();
});

test("removing an edge deletes both halves", async () => {
  await link(0, 1);
  await service.removeEdge({
    storeId,
    fromDocumentId: documents[0]!,
    toDocumentId: documents[1]!,
    relationTypeId: relation("cites"),
  });
  expect(db.client.repositories.graphEdges.listByStore(storeId)).toEqual([]);
  expect(core.edgesIn(storeId)).toEqual([]);
});

test("a failed engine removal keeps the row, flags the store and surfaces the failure", async () => {
  await link(0, 1);
  vi.spyOn(core.graph, "removeEdge").mockRejectedValueOnce(new Error("locked"));
  await expect(
    service.removeEdge({
      storeId,
      fromDocumentId: documents[0]!,
      toDocumentId: documents[1]!,
      relationTypeId: relation("cites"),
    }),
  ).rejects.toMatchObject({ code: AppErrorCode.NATIVE_ERROR });
  expect(db.client.repositories.graphEdges.listByStore(storeId)).toHaveLength(1);
  expect(db.client.repositories.vectorStores.findById(storeId)?.graphStale).toBe(true);
  expect(log).toHaveBeenCalled();
});

test("edges may not cross stores or point at a document that is not there", async () => {
  await expect(
    service.addEdge({
      storeId,
      fromDocumentId: documents[0]!,
      toDocumentId: foreign,
      relationTypeId: relation("cites"),
      weight: null,
      metadata: null,
    }),
  ).rejects.toMatchObject({ code: AppErrorCode.VALIDATION_FAILED });
  await expect(
    service.addEdge({
      storeId,
      fromDocumentId: documents[0]!,
      toDocumentId: createBrandedId(DocumentId, createId()),
      relationTypeId: relation("cites"),
      weight: null,
      metadata: null,
    }),
  ).rejects.toMatchObject({ code: AppErrorCode.NOT_FOUND });
  expect(db.client.repositories.graphEdges.listByStore(storeId)).toEqual([]);
});

test("the same relation between the same pair cannot be drawn twice", async () => {
  await link(0, 1);
  await expect(link(0, 1)).rejects.toMatchObject({ code: AppErrorCode.CONFLICT });
  await expect(link(0, 1, "user_linked")).resolves.toBeDefined();
  expect(db.client.repositories.graphEdges.listByStore(storeId)).toHaveLength(2);
});

test("neighbors, shortest path and k-hop map onto the engine calls", async () => {
  await link(0, 1);
  await link(1, 2, "derived_from");
  const neighbors = await service.neighbors({
    storeId,
    documentId: documents[1]!,
    direction: "incoming",
  });
  expect(neighbors).toEqual([
    {
      node: { id: documents[0], kind: "document" },
      edge: {
        from: documents[0],
        to: documents[1],
        relation: "cites",
        weight: null,
        metadata: null,
      },
      direction: "incoming",
    },
  ]);

  const path = await service.shortestPath({
    storeId,
    fromDocumentId: documents[0]!,
    toDocumentId: documents[2]!,
    direction: "outgoing",
  });
  expect(path?.nodes.map((node) => node.id)).toEqual(documents);
  expect(path?.edges.map((edge) => edge.relation)).toEqual(["cites", "derived_from"]);

  const filtered = await service.shortestPath({
    storeId,
    fromDocumentId: documents[0]!,
    toDocumentId: documents[2]!,
    relation: "cites",
    direction: "outgoing",
  });
  expect(filtered).toBeNull();

  const subgraph = await service.kHopSubgraph({ storeId, documentId: documents[0]!, hops: 1 });
  expect(subgraph.root).toBe(documents[0]);
  expect(subgraph.nodes.map((entry) => entry.hops)).toEqual([0, 1]);
  expect(contract["graphRelations.kHopSubgraph"].output.parse(subgraph)).toEqual(subgraph);
});

test("an unknown relation name is rejected before the engine sees the query", async () => {
  const neighbors = vi.spyOn(core.graph, "neighbors");
  await expect(
    service.neighbors({ storeId, documentId: documents[0]!, relation: "invented" }),
  ).rejects.toMatchObject({ code: AppErrorCode.NOT_FOUND });
  expect(neighbors).not.toHaveBeenCalled();
});

test("reconcile flags drift when the ledger loses a row the engine still holds", async () => {
  await link(0, 1);
  await expect(service.reconcile(storeId)).resolves.toEqual({
    storeId,
    recordedEdges: 1,
    engineEdges: 1,
    graphStale: false,
  });
  expect(db.client.repositories.vectorStores.findById(storeId)?.graphStale).toBe(false);

  const [row] = db.client.repositories.graphEdges.listByStore(storeId);
  db.client.repositories.graphEdges.remove(row!.id);
  const drifted = await service.reconcile(storeId);
  expect(drifted).toEqual({ storeId, recordedEdges: 0, engineEdges: 1, graphStale: true });
  expect(db.client.repositories.vectorStores.findById(storeId)?.graphStale).toBe(true);
  expect(contract["graphRelations.reconcile"].output.parse(drifted)).toEqual(drifted);
});

test("reconcile clears the flag once the two halves agree again", async () => {
  await link(0, 1);
  const [row] = db.client.repositories.graphEdges.listByStore(storeId);
  db.client.repositories.graphEdges.remove(row!.id);
  await service.reconcile(storeId);
  await core.graph.removeEdge(storeId, documents[0]!, documents[1]!, "cites");
  await expect(service.reconcile(storeId)).resolves.toMatchObject({ graphStale: false });
  expect(db.client.repositories.vectorStores.findById(storeId)?.graphStale).toBe(false);
});

test("a store is hydrated from the ledger once, before the first traversal", async () => {
  const edges = db.client.repositories.graphEdges;
  edges.link({
    id: createId(),
    storeId,
    fromDocumentId: documents[0]!,
    toDocumentId: documents[1]!,
    relationTypeId: relation("co_occurs"),
    weight: 0.25,
    metadata: null,
    createdAt: 1,
  });
  expect(core.hydrations).toBe(0);
  const neighbors = await service.neighbors({ storeId, documentId: documents[0]! });
  expect(neighbors.map((entry) => entry.edge.relation)).toEqual(["co_occurs"]);
  expect(neighbors[0]?.edge.weight).toBe(0.25);
  await service.neighbors({ storeId, documentId: documents[1]! });
  expect(core.hydrations).toBe(1);
  await expect(service.reconcile(storeId)).resolves.toMatchObject({ graphStale: false });
});

test("handlers map the graph channels onto the service", async () => {
  const handlers = createGraphRelationHandlers(service);
  const edge = await handlers["graphRelations.addEdge"]({
    storeId,
    fromDocumentId: documents[0]!,
    toDocumentId: documents[1]!,
    relationTypeId: relation("cites"),
    weight: 1,
    metadata: null,
  });
  expect(edge.weight).toBe(1);
  await expect(
    handlers["graphRelations.shortestPath"]({
      storeId,
      fromDocumentId: documents[0]!,
      toDocumentId: documents[1]!,
      direction: "both",
    }),
  ).resolves.toMatchObject({ edges: [{ relation: "cites" }] });
  await expect(
    handlers["graphRelations.removeEdge"]({
      storeId,
      fromDocumentId: documents[0]!,
      toDocumentId: documents[1]!,
      relationTypeId: relation("cites"),
    }),
  ).resolves.toEqual({ id: edge.id, removed: true });
  expect(core.edgesIn(storeId)).toEqual([]);
});
