import assert from "node:assert/strict";
import { test } from "vitest";
import { AppErrorCode } from "@zvs/shared";
import { RustCore } from "../src/host/drivers/rust/RustCore.ts";
import type { NativeAddon } from "../src/host/drivers/rust/addon.ts";
import type {
  GraphNeighbor,
  GraphPath,
  GraphSubgraph,
} from "../src/host/drivers/rust/graphTypes.ts";

const edge = { from: "a", to: "b", relation: "cites", weight: 0.5, metadata: { note: "п" } };
const neighbor: GraphNeighbor = { node: { id: "b", kind: "chunk" }, edge, direction: "outgoing" };
const path: GraphPath = {
  nodes: [
    { id: "a", kind: "document" },
    { id: "b", kind: "chunk" },
  ],
  edges: [edge],
};
const subgraph: GraphSubgraph = {
  root: "a",
  nodes: [
    { node: { id: "a", kind: "document" }, hops: 0 },
    { node: { id: "b", kind: "chunk" }, hops: 1 },
  ],
  edges: [edge],
};

const graphStats = { nodeCount: 0, edgeCount: 0, documentCount: 0, chunkCount: 0 };

function native(overrides: Partial<NativeAddon> = {}): NativeAddon {
  return {
    async chunkText() {
      return [];
    },
    async hashBytes() {
      return "hash";
    },
    async vectorCall() {
      throw new Error("unused");
    },
    async vectorBeginUpsert() {
      throw new Error("unused");
    },
    async vectorCancel() {},
    async vectorRelease() {},
    async graphHydrate() {
      return JSON.stringify(graphStats);
    },
    async graphStats() {
      return JSON.stringify(graphStats);
    },
    async graphAddNode() {
      return true;
    },
    async graphAddEdge() {
      return true;
    },
    async graphRemoveEdge() {
      return true;
    },
    async graphRemoveNode() {
      return 2;
    },
    async graphNeighbors() {
      return JSON.stringify([neighbor]);
    },
    async graphShortestPath() {
      return JSON.stringify(path);
    },
    async graphKHopSubgraph() {
      return JSON.stringify(subgraph);
    },
    ...overrides,
  };
}

test("graph namespace round-trips every export through the addon", async () => {
  const calls: unknown[][] = [];
  const core = new RustCore(() =>
    native({
      async graphAddNode(...args) {
        calls.push(["addNode", ...args]);
        return true;
      },
      async graphAddEdge(...args) {
        calls.push(["addEdge", ...args]);
        return false;
      },
      async graphRemoveEdge(...args) {
        calls.push(["removeEdge", ...args]);
        return true;
      },
      async graphRemoveNode(...args) {
        calls.push(["removeNode", ...args]);
        return 3;
      },
      async graphNeighbors(...args) {
        calls.push(["neighbors", ...args]);
        return JSON.stringify([neighbor]);
      },
      async graphShortestPath(...args) {
        calls.push(["shortestPath", ...args]);
        return JSON.stringify(path);
      },
      async graphKHopSubgraph(...args) {
        calls.push(["kHopSubgraph", ...args]);
        return JSON.stringify(subgraph);
      },
    }),
  );

  assert.equal(await core.graph.addNode("store", { id: " a ", kind: "document" }), true);
  assert.equal(await core.graph.addEdge("store", edge), false);
  assert.equal(await core.graph.removeEdge("store", "a", "b", "cites"), true);
  assert.equal(await core.graph.removeNode("store", "a"), 3);
  assert.deepEqual(await core.graph.neighbors("store", { id: "a" }), [neighbor]);
  assert.deepEqual(await core.graph.shortestPath("store", { from: "a", to: "b" }), path);
  assert.deepEqual(await core.graph.kHopSubgraph("store", { id: "a", hops: 2 }), subgraph);

  assert.deepEqual(calls, [
    ["addNode", "store", JSON.stringify({ id: "a", kind: "document" })],
    ["addEdge", "store", JSON.stringify(edge)],
    ["removeEdge", "store", "a", "b", "cites"],
    ["removeNode", "store", "a"],
    ["neighbors", "store", JSON.stringify({ id: "a", direction: "both" })],
    ["shortestPath", "store", JSON.stringify({ from: "a", to: "b", direction: "both" })],
    ["kHopSubgraph", "store", JSON.stringify({ id: "a", hops: 2, direction: "both" })],
  ]);
});

test("edge metadata and optional weight cross the boundary without double encoding", async () => {
  let payload = "";
  const core = new RustCore(() =>
    native({
      async graphAddEdge(_graphId, value) {
        payload = value;
        return true;
      },
    }),
  );
  await core.graph.addEdge("store", { from: "a", to: "b", relation: "user_linked" });
  assert.deepEqual(JSON.parse(payload), {
    from: "a",
    to: "b",
    relation: "user_linked",
    weight: null,
    metadata: null,
  });
  await core.graph.addEdge("store", { ...edge, metadata: { nested: { list: [1, "два"] } } });
  const decoded = JSON.parse(payload) as { metadata: unknown };
  assert.deepEqual(decoded.metadata, { nested: { list: [1, "два"] } });
});

test("absent graph results survive the boundary as null and empty collections", async () => {
  const core = new RustCore(() =>
    native({
      async graphRemoveNode() {
        return null;
      },
      async graphShortestPath() {
        return "null";
      },
      async graphNeighbors() {
        return "[]";
      },
    }),
  );
  assert.equal(await core.graph.removeNode("store", "gone"), null);
  assert.equal(await core.graph.shortestPath("store", { from: "a", to: "b" }), null);
  assert.deepEqual(await core.graph.neighbors("store", { id: "a" }), []);
});

test("graph calls reject bad arguments before reaching the addon", async () => {
  const refuse = () => {
    throw new Error("must not call");
  };
  const core = new RustCore(() =>
    native({
      graphAddNode: refuse,
      graphAddEdge: refuse,
      graphKHopSubgraph: refuse,
      graphNeighbors: refuse,
      graphRemoveNode: refuse,
    }),
  );
  for (const call of [
    () => core.graph.addNode(" ", { id: "a", kind: "document" }),
    () => core.graph.addNode("store", { id: "", kind: "document" }),
    () => core.graph.addEdge("store", { from: "a", to: "b", relation: " " }),
    () => core.graph.addEdge("store", { from: "a", to: "b", relation: "r", weight: Infinity }),
    () => core.graph.removeNode("store", ""),
    () => core.graph.neighbors("store", { id: "a", direction: "sideways" as "both" }),
    () => core.graph.kHopSubgraph("store", { id: "a", hops: 9 }),
    () => core.graph.kHopSubgraph("store", { id: "a", hops: 1, budget: 0 }),
  ]) {
    await assert.rejects(call(), { code: AppErrorCode.VALIDATION_FAILED });
  }
});

test("graph failures map native codes, including a panic and a passed traversal bound", async () => {
  for (const code of ["LIMIT_EXCEEDED", "VALIDATION_FAILED", "NATIVE_ERROR", "future-code"]) {
    const failure = new Error(JSON.stringify({ code, message: "native detail" }));
    const throws = () => {
      throw failure;
    };
    const core = new RustCore(() =>
      native({
        graphAddNode: throws,
        graphAddEdge: throws,
        graphRemoveEdge: throws,
        graphRemoveNode: throws,
        graphNeighbors: throws,
        graphShortestPath: throws,
        graphKHopSubgraph: throws,
      }),
    );
    const expected = code === "future-code" ? AppErrorCode.NATIVE_ERROR : code;
    for (const call of [
      () => core.graph.addNode("store", { id: "a", kind: "document" }),
      () => core.graph.addEdge("store", edge),
      () => core.graph.removeEdge("store", "a", "b", "cites"),
      () => core.graph.removeNode("store", "a"),
      () => core.graph.neighbors("store", { id: "a" }),
      () => core.graph.shortestPath("store", { from: "a", to: "b" }),
      () => core.graph.kHopSubgraph("store", { id: "a", hops: 1 }),
    ]) {
      await assert.rejects(call(), { code: expected, cause: failure });
    }
  }
});

test("malformed native graph responses become native errors, not raw parse failures", async () => {
  const core = new RustCore(() =>
    native({
      async graphNeighbors() {
        return "not json";
      },
      async graphKHopSubgraph() {
        return JSON.stringify({ root: "a" });
      },
    }),
  );
  await assert.rejects(core.graph.neighbors("store", { id: "a" }), {
    code: AppErrorCode.NATIVE_ERROR,
  });
  await assert.rejects(core.graph.kHopSubgraph("store", { id: "a", hops: 1 }), {
    code: AppErrorCode.NATIVE_ERROR,
  });
});
