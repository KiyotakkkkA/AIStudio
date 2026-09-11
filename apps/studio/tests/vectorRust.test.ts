import assert from "node:assert/strict";
import { test } from "vitest";
import { AppErrorCode } from "@zvs/shared";
import { RustCore } from "../src/host/drivers/rust/RustCore.ts";
import type { NativeAddon } from "../src/host/drivers/rust/addon.ts";
import { defaultVectorStore } from "../src/host/drivers/rust/vectorTypes.ts";
import { vectorStorePath, vectorStoresDir } from "../src/host/platform/paths.ts";
import { STUDIO_ROOT } from "../../../test/helpers/paths.ts";
import { join } from "node:path";

const stats = {
  rowCount: 0,
  dimension: 1024,
  metric: "cosine",
  onDiskBytes: 123,
  indexType: "FLAT",
};

function native(overrides: Partial<NativeAddon> = {}): NativeAddon {
  return {
    async chunkText() {
      return [];
    },
    async hashBytes() {
      return "hash";
    },
    async vectorCall() {
      return JSON.stringify(stats);
    },
    async vectorBeginUpsert() {
      return "operation";
    },
    async vectorCancel() {},
    async vectorRelease() {},
    ...overrides,
  };
}

test("vector wrapper transports typed operations and validates native results", async () => {
  const requests: unknown[] = [];
  const core = new RustCore(() =>
    native({
      async vectorCall(request) {
        const value = JSON.parse(request) as { operation: string };
        requests.push(value);
        if (value.operation.startsWith("delete")) return "null";
        if (value.operation === "search") return "[]";
        return JSON.stringify(stats);
      },
    }),
  );
  assert.deepEqual(defaultVectorStore, { dimension: 1024, metric: "cosine" });
  assert.deepEqual(await core.createVectorIndex("store", 1024), stats);
  assert.deepEqual(await core.openVectorIndex("store"), stats);
  assert.deepEqual(await core.vectorStats("store"), stats);
  assert.deepEqual(await core.searchVectors("store", [1, 0], 3, 0.8, "chunk_index > 2"), []);
  await core.deleteVectorsByIds("store", ["a"]);
  await core.deleteVectorsBySource("store", "doc");
  assert.deepEqual(requests, [
    { operation: "create", path: "store", dimension: 1024, metric: "cosine" },
    { operation: "open", path: "store" },
    { operation: "stats", path: "store" },
    {
      operation: "search",
      path: "store",
      vector: [1, 0],
      k: 3,
      minScore: 0.8,
      filter: "chunk_index > 2",
    },
    { operation: "deleteByIds", path: "store", ids: ["a"] },
    { operation: "deleteBySource", path: "store", documentId: "doc" },
  ]);
  await assert.rejects(
    new RustCore(() =>
      native({
        async vectorCall() {
          return "{}";
        },
      }),
    ).vectorStats("store"),
    { code: AppErrorCode.NATIVE_ERROR },
  );
  await assert.rejects(
    new RustCore(() =>
      native({
        async vectorCall() {
          throw new Error(
            JSON.stringify({ code: "VALIDATION_FAILED", message: "dimension mismatch" }),
          );
        },
      }),
    ).createVectorIndex("store", 3),
    { code: AppErrorCode.VALIDATION_FAILED },
  );
});

test("upsert cancels active native work and releases operation resources", async () => {
  const controller = new AbortController();
  const events: string[] = [];
  let rejectWork: ((error: Error) => void) | undefined;
  const addon = native({
    async vectorCall(request) {
      assert.deepEqual(JSON.parse(request), {
        operation: "upsert",
        path: "store",
        rows: [],
        operationId: "operation",
      });
      const pending = new Promise<string>((_, reject) => {
        rejectWork = reject;
      });
      controller.abort();
      return pending;
    },
    async vectorCancel(id) {
      events.push(`cancel:${id}`);
      rejectWork?.(new Error(JSON.stringify({ code: "RUN_CANCELLED", message: "cancelled" })));
    },
    async vectorRelease(id) {
      events.push(`release:${id}`);
    },
  });
  await assert.rejects(new RustCore(() => addon).upsertVectors("store", [], controller.signal), {
    code: AppErrorCode.RUN_CANCELLED,
  });
  assert.deepEqual(events, ["cancel:operation", "release:operation"]);
  let loaded = false;
  await assert.rejects(
    new RustCore(() => {
      loaded = true;
      return addon;
    }).upsertVectors("store", [], controller.signal),
    { code: AppErrorCode.RUN_CANCELLED },
  );
  assert.equal(loaded, false);
});

test("abort during token allocation is honoured before writes", async () => {
  const controller = new AbortController();
  const events: string[] = [];
  const core = new RustCore(() =>
    native({
      async vectorBeginUpsert() {
        controller.abort();
        return "early";
      },
      async vectorCall() {
        assert.fail("must not write");
      },
      async vectorCancel(id) {
        events.push(`cancel:${id}`);
      },
      async vectorRelease(id) {
        events.push(`release:${id}`);
      },
    }),
  );
  await assert.rejects(core.upsertVectors("store", [], controller.signal), {
    code: AppErrorCode.RUN_CANCELLED,
  });
  assert.deepEqual(events, ["cancel:early", "release:early"]);
});

test("host resolves vector directories and rejects traversal store IDs", () => {
  const directory = vectorStoresDir({
    userData: STUDIO_ROOT,
    resources: "",
    appRoot: "",
    packaged: false,
  });
  assert.equal(vectorStorePath(directory, "store-1"), join(STUDIO_ROOT, "vectors/store-1"));
  for (const id of ["..", "../other", "a/b", "a\\b", ""])
    assert.throws(() => vectorStorePath(directory, id));
});
