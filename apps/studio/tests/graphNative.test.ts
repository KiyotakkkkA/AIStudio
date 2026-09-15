import { expect, test } from "vitest";
import { join } from "node:path";
import { createBrandedId, DocumentId, GraphRelationTypeId, VectorStoreId } from "@zvs/shared";
import { temporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { STUDIO_ROOT } from "../../../test/helpers/paths.ts";
import { createId } from "../src/host/platform/ids.ts";
import { RustCore } from "../src/host/drivers/rust/RustCore.ts";
import { nativeTargetTriple } from "../src/host/platform/paths.ts";
import { GraphService } from "../src/host/services/GraphService.ts";
import { createGraphRelationHandlers } from "../src/host/ipc/graphRelations.ts";

test("channels draw edges through the real graph engine and reconcile catches a lost row", async () => {
  const db = temporaryDatabase();
  try {
    const core = RustCore.fromPaths({
      nativeAddonPath: join(STUDIO_ROOT, "resources/native", nativeTargetTriple(), "zvs-core.node"),
    });
    const provider = db.client.repositories.providers.create({
      name: "Native graph smoke",
      kind: "ollama",
      baseUrl: "http://localhost:11434",
      capabilities: ["embedding"],
      createdAt: 1,
      updatedAt: 1,
    });
    const created = db.client.repositories.vectorStores.create({
      id: createId(),
      name: "Native graph",
      embeddingProviderId: provider.id,
      embeddingModelId: "seed-embedding",
      dimension: 3,
      metric: "cosine",
      chunkSize: 256,
      chunkOverlap: 32,
      createdAt: 1,
      updatedAt: 1,
    });
    const store = { ...created, id: createBrandedId(VectorStoreId, created.id) };
    const documents = ["a.md", "b.md", "c.md"].map((sourcePath) =>
      createBrandedId(
        DocumentId,
        db.client.repositories.vectorDocuments.recordIndexed({
          id: createId(),
          storeId: store.id,
          sourcePath,
          contentHash: sourcePath,
          chunkCount: 1,
          bytes: 10,
          indexedAt: 1,
        }).id,
      ),
    );
    const relations = Object.fromEntries(
      db.client.repositories.graphRelations
        .list()
        .map((row) => [row.name, createBrandedId(GraphRelationTypeId, row.id)]),
    );
    const handlers = createGraphRelationHandlers(new GraphService({ data: db.client, core }));

    await handlers["graphRelations.addEdge"]({
      storeId: store.id,
      fromDocumentId: documents[0]!,
      toDocumentId: documents[1]!,
      relationTypeId: relations["cites"]!,
      weight: 0.75,
      metadata: { note: "смок" },
    });
    await handlers["graphRelations.addEdge"]({
      storeId: store.id,
      fromDocumentId: documents[1]!,
      toDocumentId: documents[2]!,
      relationTypeId: relations["derived_from"]!,
      weight: null,
      metadata: null,
    });

    const edges = db.client.repositories.graphEdges.listByStore(store.id);
    expect(edges).toHaveLength(2);

    const neighbors = await handlers["graphRelations.neighbors"]({
      storeId: store.id,
      documentId: documents[1]!,
      direction: "both",
    });
    expect(neighbors.map((entry) => [entry.node.id, entry.direction])).toEqual([
      [documents[0], "incoming"],
      [documents[2], "outgoing"],
    ]);
    expect(neighbors[0]?.edge).toMatchObject({ weight: 0.75, metadata: { note: "смок" } });

    const path = await handlers["graphRelations.shortestPath"]({
      storeId: store.id,
      fromDocumentId: documents[0]!,
      toDocumentId: documents[2]!,
      direction: "outgoing",
    });
    expect(path?.nodes.map((node) => node.id)).toEqual(documents);

    await expect(handlers["graphRelations.reconcile"]({ id: store.id })).resolves.toEqual({
      storeId: store.id,
      recordedEdges: 2,
      engineEdges: 2,
      graphStale: false,
    });

    db.client.repositories.graphEdges.remove(edges[0]!.id);
    await expect(handlers["graphRelations.reconcile"]({ id: store.id })).resolves.toEqual({
      storeId: store.id,
      recordedEdges: 1,
      engineEdges: 2,
      graphStale: true,
    });
    expect(db.client.repositories.vectorStores.findById(store.id)?.graphStale).toBe(true);
  } finally {
    db.dispose();
  }
});
