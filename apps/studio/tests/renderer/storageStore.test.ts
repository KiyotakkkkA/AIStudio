import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { AppErrorCode, contract, type Contract, type StorageUsageDto } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import StorageStore from "../../src/renderer/features/preferences/StorageStore.ts";
import {
  STORAGE_CACHE_BUSY,
  STORAGE_UNAVAILABLE,
  clearedCopy,
  storageSize,
} from "../../src/renderer/features/preferences/storageCopy.ts";

function usage(cache: number): StorageUsageDto {
  const categories = [
    { category: "conversations" as const, bytes: 300 },
    { category: "cache" as const, bytes: cache },
    { category: "toolchain" as const, bytes: 5_000 },
  ];
  return {
    totalBytes: categories.reduce((sum, item) => sum + item.bytes, 0),
    categories,
    measuredAt: 1_760_000_000_000,
  };
}

test("usage loads cached, refreshes on demand and lists the largest category first", async () => {
  const bridge = createFakeBridge<Contract>().handle("storage.usage", () => usage(40));
  const store = new StorageStore(createIpcClient(contract, bridge));

  await store.load();
  await store.load(true);

  expect(bridge.calls.map((call) => call.payload)).toEqual([{ refresh: false }, { refresh: true }]);
  expect(store.categories.map((item) => item.category)).toEqual([
    "toolchain",
    "conversations",
    "cache",
  ]);
  expect(store.error).toBeNull();
});

test("category parts are listed largest first and expand per category", async () => {
  const bridge = createFakeBridge<Contract>().handle("storage.usage", () => ({
    totalBytes: 30,
    categories: [
      {
        category: "downloads" as const,
        bytes: 20,
        parts: [
          { part: "mcp" as const, bytes: 4 },
          { part: "models" as const, bytes: 16 },
        ],
      },
      { category: "logs" as const, bytes: 10 },
    ],
    measuredAt: 1_760_000_000_000,
  }));
  const store = new StorageStore(createIpcClient(contract, bridge));
  await store.load();

  expect(store.categories.map((item) => item.parts.map((part) => part.part))).toEqual([
    ["models", "mcp"],
    [],
  ]);
  expect(store.isExpanded("downloads")).toBe(false);
  store.toggle("downloads");
  expect(store.isExpanded("downloads")).toBe(true);
  store.toggle("downloads");
  expect(store.isExpanded("downloads")).toBe(false);
});

test("clearing the cache reports the freed space and re-measures", async () => {
  let cache = 2_621_440;
  const bridge = createFakeBridge<Contract>()
    .handle("storage.usage", () => usage(cache))
    .handle("storage.clearCache", () => {
      const removedBytes = cache;
      cache = 0;
      return { removedBytes };
    });
  const store = new StorageStore(createIpcClient(contract, bridge));
  await store.load();

  await store.clearCache();

  expect(store.notice).toBe(clearedCopy(2_621_440));
  expect(store.usage?.totalBytes).toBe(5_300);
  expect(bridge.calls.at(-1)).toEqual({ channel: "storage.usage", payload: { refresh: true } });
});

test("a refused or failed request maps to copy by error code", async () => {
  const bridge = createFakeBridge<Contract>()
    .fail("storage.clearCache", AppErrorCode.CONFLICT, "busy")
    .fail("storage.usage", AppErrorCode.NATIVE_ERROR, "io");
  const store = new StorageStore(createIpcClient(contract, bridge));

  await store.clearCache();
  expect(store.error).toBe(STORAGE_CACHE_BUSY);
  expect(store.notice).toBeNull();

  await store.load();
  expect(store.error).toBe(STORAGE_UNAVAILABLE);
});

test("sizes use Russian units and the ru-RU decimal separator", () => {
  expect(storageSize(0)).toBe("0 Б");
  expect(storageSize(512)).toBe("512 Б");
  expect(storageSize(2_621_440)).toBe(`${(2.5).toLocaleString("ru-RU")} МБ`);
  expect(storageSize(317 * 1024 ** 2)).toBe("317 МБ");
  expect(clearedCopy(0)).not.toBe(clearedCopy(1));
});
