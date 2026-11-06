import { describe, expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { AppErrorCode, contract, type BrowserExtensionDto, type Contract } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import BrowserExtensionsStore from "../../src/renderer/features/browser/BrowserExtensionsStore.ts";
import {
  EXTENSION_INVALID,
  EXTENSIONS_FULL,
  extensionCountLabel,
} from "../../src/renderer/features/browser/browserExtensionsCopy.ts";

const UBLOCK: BrowserExtensionDto = {
  id: "ublock-origin",
  name: "uBlock Origin",
  version: "1.62.0",
  description: "",
  enabled: true,
};

describe("BrowserExtensionsStore", () => {
  test("adds, turns off and removes an extension", async () => {
    let stored: BrowserExtensionDto[] = [];
    const bridge = createFakeBridge<Contract>()
      .handle("browserExtensions.list", () => ({ items: stored }))
      .handle("browserExtensions.add", () => {
        stored = [UBLOCK];
        return { items: stored };
      })
      .handle("browserExtensions.setEnabled", ({ id, enabled }) => {
        stored = stored.map((item) => (item.id === id ? { ...item, enabled } : item));
        return { items: stored };
      })
      .handle("browserExtensions.remove", ({ id }) => {
        stored = stored.filter((item) => item.id !== id);
        return { items: stored };
      });
    const store = new BrowserExtensionsStore(createIpcClient(contract, bridge));
    await store.load();
    expect(store.items).toEqual([]);
    await store.add();
    expect(store.items).toEqual([UBLOCK]);
    await store.setEnabled(UBLOCK.id, false);
    expect(store.items[0]?.enabled).toBe(false);
    await store.remove(UBLOCK.id);
    expect(store.items).toEqual([]);
  });

  test("maps a folder without a manifest and a full list to copy", async () => {
    const bridge = createFakeBridge<Contract>();
    bridge.fail("browserExtensions.add", AppErrorCode.VALIDATION_FAILED, "no manifest");
    const store = new BrowserExtensionsStore(createIpcClient(contract, bridge));
    await store.add();
    expect(store.error).toBe(EXTENSION_INVALID);
    bridge.fail("browserExtensions.add", AppErrorCode.CONFLICT, "full");
    await store.add();
    expect(store.error).toBe(EXTENSIONS_FULL);
    expect(store.busy).toBe(false);
  });

  test("counts extensions in Russian", () => {
    expect(extensionCountLabel(2)).toBe("2 расширения");
    expect(extensionCountLabel(5)).toBe("5 расширений");
  });
});
