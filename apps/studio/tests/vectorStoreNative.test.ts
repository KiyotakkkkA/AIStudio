import { expect, test } from "vitest";
import { existsSync } from "node:fs";
import { rm } from "node:fs/promises";
import { join } from "node:path";
import { CreateVectorStoreInput } from "@zvs/shared";
import { temporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { STUDIO_ROOT, assertTemporaryPath } from "../../../test/helpers/paths.ts";
import { createFakeDriver } from "../../../test/helpers/FakeDriver.ts";
import { RustCore } from "../src/host/drivers/rust/RustCore.ts";
import { nativeTargetTriple } from "../src/host/platform/paths.ts";
import { VectorStoreService } from "../src/host/services/VectorStoreService.ts";
import { createVectorStoreHandlers } from "../src/host/ipc/vectorStores.ts";

test("channels create, search a manually seeded LanceDB table, detect loss and remove both halves", async () => {
  const db = temporaryDatabase();
  try {
    const core = RustCore.fromPaths({
      nativeAddonPath: join(STUDIO_ROOT, "resources/native", nativeTargetTriple(), "zvs-core.node"),
    });
    const provider = db.client.repositories.providers.create({
      name: "Native smoke",
      kind: "ollama",
      baseUrl: "http://localhost:11434",
      capabilities: ["embedding"],
      createdAt: 1,
      updatedAt: 1,
    });
    const fake = createFakeDriver();
    const service = new VectorStoreService({
      data: db.client,
      core,
      directory: db.directory,
      drivers: {
        ephemeralDriver: async () => ({
          ...fake,
          embedding: {
            dimensions: () => 3,
            capabilities: () => fake.text.capabilities(),
            listModels: async () => [],
            embed: async () => [new Float32Array([1, 0, 0])],
          },
        }),
      },
    });
    const handlers = createVectorStoreHandlers(service);
    const input = CreateVectorStoreInput.parse({
      name: "Native",
      embeddingProviderId: provider.id,
      embeddingModelId: "seed-embedding",
      dimension: 3,
    });
    const store = await handlers["vectorStores.create"](input);
    const path = assertTemporaryPath(join(db.directory, store.id));
    expect(existsSync(join(path, "vectors.lance"))).toBe(true);
    expect(db.client.repositories.vectorStores.findById(store.id)).toBeDefined();
    await core.upsertVectors(path, [
      {
        id: "a",
        vector: [1, 0, 0],
        payload: { text: "A" },
        documentId: "doc-a",
        chunkIndex: 0,
        path: "a.md",
      },
      {
        id: "b",
        vector: [0.9, 0.1, 0],
        payload: { text: "B" },
        documentId: "doc-a",
        chunkIndex: 1,
        path: "a.md",
      },
      {
        id: "c",
        vector: [0, 1, 0],
        payload: { text: "C" },
        documentId: "doc-b",
        chunkIndex: 0,
        path: "b.md",
      },
    ]);
    const hits = await handlers["vectorStores.search"]({
      storeId: store.id,
      query: "A",
      k: 2,
      minScore: 0.5,
    });
    expect(hits.map((hit) => hit.id)).toEqual(["a", "b"]);
    expect((await handlers["vectorStores.list"](undefined))[0]?.status).toBe("stale");
    expect(await handlers["vectorStores.get"]({ id: store.id })).toMatchObject({
      status: "healthy",
      documents: 2,
      vectors: 3,
    });
    await rm(path, { recursive: true });
    expect((await handlers["vectorStores.get"]({ id: store.id })).status).toBe("broken");
    await handlers["vectorStores.remove"]({ id: store.id });
    const second = await handlers["vectorStores.create"](input);
    await handlers["vectorStores.remove"]({ id: second.id });
    expect(existsSync(join(db.directory, second.id))).toBe(false);
    expect(await service.list()).toEqual([]);
  } finally {
    db.dispose();
  }
}, 30000);
