import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createRequire } from "node:module";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath, URL } from "node:url";
import { Buffer } from "node:buffer";
import { setInterval, clearInterval } from "node:timers";
import { mkdtempSync, rmSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";

const root = fileURLToPath(new URL("../../", import.meta.url));
const triple = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  .match(/^host: (.+)$/m)[1]
  .trim();
const directory = join(root, "apps/studio/resources/native", triple);
const require = createRequire(import.meta.url);
const addon = require(join(directory, "zvs-core.node"));

test("release addon chunks owned Unicode text and hashes binary content asynchronously", async () => {
  assert.equal(addon.debugPanic, undefined);
  const pending = addon.chunkText("Привет 🌍 мир", { size: 2, overlap: 1 });
  assert.ok(pending instanceof Promise);
  const chunks = await pending;
  assert.deepEqual(
    chunks.map((chunk) => chunk.text),
    ["Привет 🌍", "🌍 мир"],
  );
  for (const chunk of chunks) {
    assert.equal(
      Buffer.from("Привет 🌍 мир").subarray(chunk.byteStart, chunk.byteEnd).toString(),
      chunk.text,
    );
    assert.equal(chunk.tokenCount, 2);
  }
  assert.deepEqual(await addon.chunkText("", { size: 2, overlap: 0 }), []);
  const hash = addon.hashBytes(Buffer.from("abc"));
  assert.ok(hash instanceof Promise);
  assert.equal(await hash, "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85");
  await assert.rejects(addon.chunkText("invalid", { size: 0, overlap: 0 }), (error) => {
    assert.equal(JSON.parse(error.message).code, "VALIDATION_FAILED");
    return true;
  });
});

test("debug panic rejects and the same process can make another real native call", async () => {
  const debug = require(join(directory, "debug/zvs-core.node"));
  const pending = debug.debugPanic();
  assert.ok(pending instanceof Promise);
  await assert.rejects(pending, (error) => {
    assert.equal(JSON.parse(error.message).code, "NATIVE_ERROR");
    return true;
  });
  assert.equal((await debug.chunkText("still alive", { size: 1, overlap: 0 })).length, 2);
});

test("native work leaves the JavaScript event loop responsive", async () => {
  let ticks = 0;
  const timer = setInterval(() => {
    ticks += 1;
  }, 1);
  try {
    await addon.chunkText("word ".repeat(500_000), { size: 256, overlap: 32 });
    assert.ok(ticks > 0);
  } finally {
    clearInterval(timer);
  }
});

test("vector addon persists hundreds of vectors, replaces IDs, filters, deletes and reports stats", async () => {
  const path = mkdtempSync(join(tmpdir(), "zvs-vector-native-"));
  const call = async (request) =>
    JSON.parse(await addon.vectorCall(JSON.stringify({ path, ...request })));
  try {
    const created = await call({ operation: "create", dimension: 16, metric: "cosine" });
    assert.equal(created.rowCount, 0);
    assert.equal(created.dimension, 16);
    assert.equal(created.indexType, "FLAT");
    let seed = 19;
    const random = () => {
      seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
      return seed / 2 ** 32 - 0.5;
    };
    const rows = Array.from({ length: 400 }, (_, i) => ({
      id: `row-${i}`,
      vector: Array.from({ length: 16 }, random),
      payload: { text: `Фрагмент ${i}` },
      documentId: i < 200 ? "doc'1" : "doc2",
      chunkIndex: i,
      path: "C:/docs/source.txt",
    }));
    const operationId = await addon.vectorBeginUpsert();
    try {
      assert.equal(await call({ operation: "upsert", rows, operationId }), 400);
    } finally {
      await addon.vectorRelease(operationId);
    }
    assert.ok(existsSync(join(path, "vectors.lance")));
    const hits = await call({ operation: "search", vector: rows[42].vector, k: 5, minScore: 0 });
    assert.equal(hits.length, 5);
    assert.equal(hits[0].id, "row-42");
    assert.ok(hits[0].score > 0.99999);
    assert.deepEqual(hits[0].payload, rows[42].payload);
    const filtered = await call({
      operation: "search",
      vector: rows[42].vector,
      k: 5,
      minScore: 0,
      filter: "document_id = 'doc2'",
    });
    assert.ok(filtered.every((hit) => hit.documentId === "doc2"));
    const replacement = { ...rows[42], payload: { text: "updated" } };
    const updateId = await addon.vectorBeginUpsert();
    try {
      await call({ operation: "upsert", rows: [replacement], operationId: updateId });
    } finally {
      await addon.vectorRelease(updateId);
    }
    const updated = await call({
      operation: "search",
      vector: rows[42].vector,
      k: 1,
      minScore: 0.99,
    });
    assert.deepEqual(updated[0].payload, replacement.payload);
    const stats = await call({ operation: "open" });
    assert.equal(stats.rowCount, 400);
    assert.ok(stats.onDiskBytes > 0);
    await assert.rejects(call({ operation: "create", dimension: 8, metric: "cosine" }), (error) => {
      assert.equal(JSON.parse(error.message).code, "VALIDATION_FAILED");
      return true;
    });
    await call({ operation: "deleteBySource", documentId: "doc'1" });
    assert.equal((await call({ operation: "stats" })).rowCount, 200);
    await call({ operation: "deleteByIds", ids: ["row-200", "x') OR true --"] });
    assert.equal((await call({ operation: "stats" })).rowCount, 199);
    const cancelledId = await addon.vectorBeginUpsert();
    await addon.vectorCancel(cancelledId);
    try {
      await assert.rejects(
        call({ operation: "upsert", rows, operationId: cancelledId }),
        (error) => {
          assert.equal(JSON.parse(error.message).code, "RUN_CANCELLED");
          return true;
        },
      );
    } finally {
      await addon.vectorRelease(cancelledId);
    }
    assert.equal((await call({ operation: "stats" })).rowCount, 199);
  } finally {
    rmSync(path, { recursive: true, force: true });
  }
});

test("graph addon stores nodes and edges per graph id and answers every traversal", async () => {
  const store = "native-graph";
  assert.equal(
    await addon.graphAddNode(store, JSON.stringify({ id: "a", kind: "document" })),
    true,
  );
  assert.equal(
    await addon.graphAddNode(store, JSON.stringify({ id: "a", kind: "document" })),
    false,
  );
  for (const id of ["b", "c", "d"]) {
    await addon.graphAddNode(store, JSON.stringify({ id, kind: "chunk" }));
  }
  const metadata = { note: "Связь", nested: { list: [1, 2, 3] } };
  assert.equal(
    await addon.graphAddEdge(
      store,
      JSON.stringify({ from: "a", to: "b", relation: "cites", weight: 0.5, metadata }),
    ),
    true,
  );
  await addon.graphAddEdge(store, JSON.stringify({ from: "b", to: "c", relation: "cites" }));
  await addon.graphAddEdge(store, JSON.stringify({ from: "c", to: "d", relation: "co_occurs" }));

  const neighbors = JSON.parse(
    await addon.graphNeighbors(store, JSON.stringify({ id: "a", direction: "outgoing" })),
  );
  assert.equal(neighbors.length, 1);
  assert.equal(neighbors[0].node.id, "b");
  assert.equal(neighbors[0].direction, "outgoing");
  assert.equal(neighbors[0].edge.weight, 0.5);
  assert.deepEqual(neighbors[0].edge.metadata, metadata);

  const path = JSON.parse(
    await addon.graphShortestPath(store, JSON.stringify({ from: "a", to: "d" })),
  );
  assert.deepEqual(
    path.nodes.map((node) => node.id),
    ["a", "b", "c", "d"],
  );
  assert.equal(path.edges.length, 3);
  assert.equal(
    JSON.parse(
      await addon.graphShortestPath(
        store,
        JSON.stringify({ from: "a", to: "d", relation: "cites" }),
      ),
    ),
    null,
  );

  const subgraph = JSON.parse(
    await addon.graphKHopSubgraph(store, JSON.stringify({ id: "a", hops: 2 })),
  );
  assert.equal(subgraph.root, "a");
  assert.deepEqual(
    subgraph.nodes.map((entry) => [entry.node.id, entry.hops]),
    [
      ["a", 0],
      ["b", 1],
      ["c", 2],
    ],
  );

  await assert.rejects(addon.graphNeighbors("other-graph-id", '{"id":"a"}'), (error) => {
    assert.equal(JSON.parse(error.message).code, "VALIDATION_FAILED");
    return true;
  });
  await assert.rejects(
    addon.graphKHopSubgraph(store, JSON.stringify({ id: "a", hops: 2, budget: 1 })),
    (error) => {
      assert.equal(JSON.parse(error.message).code, "LIMIT_EXCEEDED");
      return true;
    },
  );

  assert.equal(await addon.graphRemoveEdge(store, "b", "c", "cites"), true);
  assert.equal(await addon.graphRemoveEdge(store, "b", "c", "cites"), false);
  assert.equal(await addon.graphRemoveNode(store, "a"), 1);
  assert.equal(await addon.graphRemoveNode(store, "a"), null);
  assert.deepEqual(JSON.parse(await addon.graphNeighbors(store, '{"id":"b"}')), []);
});

test("hydration replaces a graph from the ledger and stats report both halves", async () => {
  const store = "native-graph-hydrate";
  await addon.graphAddNode(store, JSON.stringify({ id: "stale", kind: "document" }));
  const hydrated = JSON.parse(
    await addon.graphHydrate(
      store,
      JSON.stringify({
        nodes: [
          { id: "a", kind: "document" },
          { id: "b", kind: "document" },
          { id: "c", kind: "chunk" },
        ],
        edges: [
          { from: "a", to: "b", relation: "cites", weight: 0.5, metadata: { note: "п" } },
          { from: "b", to: "c", relation: "derived_from" },
        ],
      }),
    ),
  );
  assert.deepEqual(hydrated, {
    nodeCount: 3,
    edgeCount: 2,
    documentCount: 2,
    chunkCount: 1,
  });
  assert.deepEqual(JSON.parse(await addon.graphStats(store)), hydrated);
  await assert.rejects(addon.graphNeighbors(store, '{"id":"stale"}'), (error) => {
    assert.equal(JSON.parse(error.message).code, "VALIDATION_FAILED");
    return true;
  });
  const neighbors = JSON.parse(
    await addon.graphNeighbors(store, JSON.stringify({ id: "a", direction: "outgoing" })),
  );
  assert.deepEqual(
    neighbors.map((entry) => entry.node.id),
    ["b"],
  );
  assert.deepEqual(neighbors[0].edge.metadata, { note: "п" });
  assert.equal(neighbors[0].edge.weight, 0.5);
  assert.deepEqual(JSON.parse(await addon.graphStats("native-graph-never-used")), {
    nodeCount: 0,
    edgeCount: 0,
    documentCount: 0,
    chunkCount: 0,
  });
});

test("a panic inside a graph export rejects and leaves the shared store usable", async () => {
  const debug = require(join(directory, "debug/zvs-core.node"));
  const store = "native-graph-panic";
  await debug.graphAddNode(store, JSON.stringify({ id: "a", kind: "document" }));
  await assert.rejects(debug.graphDebugPanic(store), (error) => {
    assert.equal(JSON.parse(error.message).code, "NATIVE_ERROR");
    return true;
  });
  assert.equal(await debug.graphAddNode(store, JSON.stringify({ id: "b", kind: "chunk" })), true);
  assert.equal(
    await debug.graphAddEdge(store, JSON.stringify({ from: "a", to: "b", relation: "cites" })),
    true,
  );
  assert.deepEqual(JSON.parse(await debug.graphNeighbors(store, '{"id":"a"}')).length, 1);
});
