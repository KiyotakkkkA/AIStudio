import { describe, expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  contract,
  type BrowserDownloadDto,
  type BrowserSettingsDto,
  type Contract,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import BrowserDownloadsStore from "../../src/renderer/features/browser/BrowserDownloadsStore.ts";
import BrowserSettingsStore, {
  DEFAULT_BROWSER_SETTINGS,
} from "../../src/renderer/features/browser/BrowserSettingsStore.ts";
import {
  CLEAR_DATA_DONE,
  CLEAR_DATA_FAILED,
  DOWNLOAD_ACTION_FAILED,
  downloadCountLabel,
} from "../../src/renderer/features/browser/browserDataCopy.ts";

function item(n: number, overrides: Partial<BrowserDownloadDto> = {}): BrowserDownloadDto {
  return {
    id: `0199aa22-2222-7222-8222-${String(n).padStart(12, "0")}`,
    url: "https://files.example/report.pdf",
    fileName: `report-${String(n)}.pdf`,
    path: `C:\\Downloads\\report-${String(n)}.pdf`,
    bytes: 1_024,
    state: "done",
    agent: false,
    finishedAt: 1_000 + n,
    ...overrides,
  };
}

describe("BrowserDownloadsStore", () => {
  test("lists, removes and clears the history", async () => {
    let stored = [item(2), item(1)];
    const bridge = createFakeBridge<Contract>()
      .handle("browserDownloads.list", () => ({ items: stored }))
      .handle("browserDownloads.remove", ({ id }) => {
        stored = stored.filter((entry) => entry.id !== id);
        return { items: stored };
      })
      .handle("browserDownloads.clear", () => {
        stored = [];
        return { items: stored };
      });
    const store = new BrowserDownloadsStore(createIpcClient(contract, bridge));
    await store.load();
    expect(store.items.map((entry) => entry.fileName)).toEqual(["report-2.pdf", "report-1.pdf"]);
    await store.remove(item(2).id);
    expect(store.items).toHaveLength(1);
    await store.clear();
    expect(store.items).toEqual([]);
  });

  test("reports a file that can no longer be opened", async () => {
    const bridge = createFakeBridge<Contract>()
      .handle("browserDownloads.list", () => ({ items: [item(1)] }))
      .handle("browserDownloads.reveal", () => undefined);
    bridge.fail("browserDownloads.open", AppErrorCode.NOT_FOUND, "gone");
    const store = new BrowserDownloadsStore(createIpcClient(contract, bridge));
    await store.load();
    await store.reveal(item(1).id);
    expect(store.error).toBeNull();
    await store.open(item(1).id);
    expect(store.error).toBe(DOWNLOAD_ACTION_FAILED);
    store.dismissError();
    expect(store.error).toBeNull();
  });
});

describe("BrowserSettingsStore data actions", () => {
  test("clearing reports the outcome and a picked folder replaces the settings", async () => {
    const picked: BrowserSettingsDto = { ...DEFAULT_BROWSER_SETTINGS, downloadDir: "D:\\Files" };
    let failClear = false;
    const bridge = createFakeBridge<Contract>()
      .handle("browserSettings.get", () => DEFAULT_BROWSER_SETTINGS)
      .handle("browserSettings.pickDownloadDir", () => picked)
      .handle("browserSettings.clearData", () => {
        if (failClear) throw new Error("profile");
      });
    const store = new BrowserSettingsStore(createIpcClient(contract, bridge));
    await store.load();
    await store.pickDownloadDir();
    expect(store.settings.downloadDir).toBe("D:\\Files");
    await store.clearData();
    expect(store.notice).toBe(CLEAR_DATA_DONE);
    failClear = true;
    await store.clearData();
    expect(store.error).toBe(CLEAR_DATA_FAILED);
    expect(store.clearing).toBe(false);
  });

  test("counts files in Russian", () => {
    expect(downloadCountLabel(1)).toBe("1 файл");
    expect(downloadCountLabel(3)).toBe("3 файла");
    expect(downloadCountLabel(11)).toBe("11 файлов");
  });
});
