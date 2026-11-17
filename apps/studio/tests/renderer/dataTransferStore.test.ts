import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppError,
  AppErrorCode,
  contract,
  type Contract,
  type TransferPreviewDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import DataTransferStore from "../../src/renderer/features/preferences/DataTransferStore.ts";
import {
  DATA_CORRUPT,
  DATA_DUPLICATE,
  DATA_INVALID_FILE,
} from "../../src/renderer/features/preferences/dataTransferCopy.ts";

const ID = "0199aa11-1111-7111-8111-111111111111";
const preview: TransferPreviewDto = {
  createdAt: 1,
  categories: [
    { category: "conversations", count: 2, bytes: 100 },
    { category: "connections", count: 1, bytes: 50 },
    { category: "settings", count: 1, bytes: 10 },
  ],
  warnings: ["credentials"],
};

function fixture() {
  const bridge = createFakeBridge<Contract>()
    .handle("dataTransfer.inventory", () => preview)
    .handle("dataTransfer.beginImport", () => ({ id: ID }))
    .handle("dataTransfer.appendImport", () => undefined)
    .handle("dataTransfer.inspectImport", () => preview)
    .handle("dataTransfer.disposeImport", () => undefined);
  let refreshed = 0;
  const store = new DataTransferStore(createIpcClient(contract, bridge), () => {
    refreshed += 1;
    return Promise.resolve();
  });
  return { store, bridge, refreshed: () => refreshed };
}

function file(name = "sample.dbox", bytes = new Uint8Array([1, 2, 3, 4])): File {
  return {
    name,
    size: bytes.length,
    slice: (start: number, end: number) => ({
      arrayBuffer: () => Promise.resolve(bytes.slice(start, end).buffer),
    }),
  } as File;
}

test("selection maintains connection dependencies and submits only available categories", async () => {
  const { store, bridge } = fixture();
  bridge.handle("dataTransfer.export", () => ({ path: "C:/Backups/export.dbox" }));
  await store.load();
  store.toggle("connections", false);
  expect(store.selection).toEqual(["settings"]);
  expect(store.hasDependencies).toBe(false);
  store.toggle("conversations", true);
  expect(store.selection).toEqual(["conversations", "connections", "settings"]);
  expect(store.hasDependencies).toBe(true);
  expect(store.selectedBytes).toBe(160);
  await store.export();
  expect(bridge.calls.at(-1)?.payload).toEqual({
    categories: ["conversations", "connections", "settings"],
  });
  expect(store.savedPath).toBe("C:/Backups/export.dbox");
});

test("invalid file clears old preview and never starts an upload", async () => {
  const { store, bridge } = fixture();
  await store.choose(file("sample.zip"));
  expect(store.error).toBe(DATA_INVALID_FILE);
  expect(store.preview).toBeNull();
  expect(store.canSubmit).toBe(false);
  expect(bridge.calls).toEqual([]);
});

test("upload is chunked and preview precedes explicit import", async () => {
  const { store, bridge, refreshed } = fixture();
  bridge.handle("dataTransfer.import", () => ({ categories: preview.categories }));
  await store.choose(file());
  expect(bridge.calls.map((call) => call.channel)).toEqual([
    "dataTransfer.beginImport",
    "dataTransfer.appendImport",
    "dataTransfer.inspectImport",
  ]);
  expect(bridge.calls[1]?.payload).toEqual({ id: ID, offset: 0, data: "AQIDBA==" });
  expect(store.progress).toBe(100);
  expect(store.preview).toEqual(preview);
  await store.import();
  expect(store.imported).toBe(true);
  expect(refreshed()).toBe(1);
  await store.import();
  expect(bridge.calls.filter((call) => call.channel === "dataTransfer.import")).toHaveLength(1);
});

test("large uploads use bounded chunks and contiguous offsets", async () => {
  const { store, bridge } = fixture();
  await store.choose(file("large.dbox", new Uint8Array(512 * 1024 + 1)));
  const calls = bridge.calls.filter((call) => call.channel === "dataTransfer.appendImport");
  expect(calls).toHaveLength(2);
  expect(calls[0]?.payload).toMatchObject({ offset: 0 });
  expect(calls[1]?.payload).toMatchObject({ offset: 512 * 1024, data: "AA==" });
});

test("failed inspection releases the session and exposes no importable preview", async () => {
  const { store, bridge } = fixture();
  bridge.fail("dataTransfer.inspectImport", AppErrorCode.UNSUPPORTED_FORMAT, "invalid");
  await store.choose(file());
  expect(store.error).toBe(DATA_CORRUPT);
  expect(store.preview).toBeNull();
  expect(store.canSubmit).toBe(false);
  expect(bridge.calls.at(-1)?.channel).toBe("dataTransfer.disposeImport");
});

test("a stale upload response is discarded and its session released", async () => {
  const { store, bridge } = fixture();
  let finish: ((value: { id: string }) => void) | undefined;
  bridge.handle(
    "dataTransfer.beginImport",
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const pending = store.choose(file());
  await Promise.resolve();
  await Promise.resolve();
  await store.choose(null);
  finish?.({ id: ID });
  await pending;
  expect(store.file).toBeNull();
  expect(store.preview).toBeNull();
  expect(bridge.calls.at(-1)?.channel).toBe("dataTransfer.disposeImport");
});

test("duplicate import is mapped from structured error details", async () => {
  const { store, bridge } = fixture();
  bridge.handle("dataTransfer.import", () => {
    throw new AppError(AppErrorCode.CONFLICT, "conflict", { details: { reason: "duplicate" } });
  });
  await store.choose(file());
  await store.import();
  expect(store.error).toBe(DATA_DUPLICATE);
  expect(store.imported).toBe(false);
});

test("previously imported dependencies remain satisfied without importing them twice", async () => {
  const { store, bridge } = fixture();
  bridge.handle("dataTransfer.inspectImport", () => ({
    ...preview,
    importedCategories: ["connections"],
  }));
  bridge.handle("dataTransfer.import", () => ({
    categories: preview.categories.filter((item) => item.category !== "connections"),
  }));
  await store.choose(file());
  expect(store.alreadyImported("connections")).toBe(true);
  expect(store.selection).toEqual(["conversations", "settings"]);
  expect(store.selectedCount).toBe(3);
  expect(store.selectedBytes).toBe(110);
  expect(store.canSubmit).toBe(true);
  store.toggle("connections", true);
  expect(store.selection).toEqual(["conversations", "settings"]);
  await store.import();
  expect(bridge.calls.at(-1)?.payload).toEqual({
    id: ID,
    categories: ["conversations", "settings"],
  });
});
