import { describe, expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  contract,
  type BrowserBookmarkDto,
  type BrowserHistoryEntryDto,
  type BrowserImportSourceDto,
  type Contract,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import BrowserLibraryStore from "../../src/renderer/features/browser/BrowserLibraryStore.ts";
import {
  IMPORT_GONE,
  IMPORT_LOCKED,
  historyCountLabel,
  importResultLabel,
} from "../../src/renderer/features/browser/browserLibraryCopy.ts";
import { bookmarkLabel } from "../../src/browser-ui/bookmarkCopy.ts";

function id(n: number): string {
  return `0199aa22-2222-7222-8222-${String(n).padStart(12, "0")}`;
}

function visit(n: number, title: string): BrowserHistoryEntryDto {
  return {
    id: id(n),
    url: `https://docs.example/${String(n)}`,
    title,
    visitedAt: 1_000 + n,
    importedFrom: null,
  };
}

const CHROME: BrowserImportSourceDto = {
  id: "chrome:Default",
  kind: "chrome",
  profile: "Работа",
  history: true,
  bookmarks: false,
};

describe("BrowserLibraryStore", () => {
  test("searches the history and removes a visit", async () => {
    let stored = [visit(2, "Справка"), visit(1, "Документация")];
    const bridge = createFakeBridge<Contract>()
      .handle("browserHistory.list", ({ query }) => {
        const items = stored.filter((entry) =>
          query === undefined ? true : entry.title.toLowerCase().includes(query.toLowerCase()),
        );
        return { items, total: items.length };
      })
      .handle("browserHistory.remove", ({ id: removed }) => {
        stored = stored.filter((entry) => entry.id !== removed);
      });
    const store = new BrowserLibraryStore(createIpcClient(contract, bridge));
    await store.loadHistory();
    expect(store.historyTotal).toBe(2);
    store.setQuery("  докум ");
    await store.loadHistory();
    expect(store.history.map((entry) => entry.title)).toEqual(["Документация"]);
    await store.removeVisit(id(1));
    expect(store.history).toEqual([]);
  });

  test("renames a bookmark and refuses an empty title", async () => {
    let stored: BrowserBookmarkDto[] = [
      { id: id(1), url: "https://a.example/", title: "A", folder: "", createdAt: 1 },
    ];
    const bridge = createFakeBridge<Contract>()
      .handle("browserBookmarks.list", () => ({ items: stored }))
      .handle("browserBookmarks.rename", ({ id: renamed, title }) => {
        stored = stored.map((item) => (item.id === renamed ? { ...item, title } : item));
        return { items: stored };
      });
    const store = new BrowserLibraryStore(createIpcClient(contract, bridge));
    await store.loadBookmarks();
    store.startRename(stored[0]!);
    store.setRenameDraft("   ");
    expect(store.canRename).toBe(false);
    store.setRenameDraft(" Справка ");
    await store.saveRename();
    expect(store.renamingId).toBeNull();
    expect(store.bookmarks[0]?.title).toBe("Справка");
  });

  test("imports only what the source offers and maps failures to copy", async () => {
    const bridge = createFakeBridge<Contract>()
      .handle("browserImport.sources", () => ({ sources: [CHROME] }))
      .handle("browserImport.run", () => ({ history: 3, bookmarks: 0 }));
    const store = new BrowserLibraryStore(createIpcClient(contract, bridge));
    await store.loadSources();
    expect(store.choiceOf(CHROME.id)).toEqual({ history: true, bookmarks: false });
    await store.runImport(CHROME);
    expect(store.notice).toBe(importResultLabel("chrome", { history: 3, bookmarks: 0 }));
    bridge.fail("browserImport.run", AppErrorCode.CONFLICT, "locked");
    await store.runImport(CHROME);
    expect(store.error).toBe(IMPORT_LOCKED);
    bridge.fail("browserImport.run", AppErrorCode.NOT_FOUND, "gone");
    await store.runImport(CHROME);
    expect(store.error).toBe(IMPORT_GONE);
    store.setChoice(CHROME.id, { history: false });
    const calls = bridge.calls.length;
    await store.runImport(CHROME);
    expect(bridge.calls).toHaveLength(calls);
  });

  test("labels counts and bookmarks in Russian", () => {
    expect(historyCountLabel(500, 1203)).toBe("Показаны 500 из 1203 страницы");
    expect(historyCountLabel(2, 2)).toBe("2 страницы");
    expect(importResultLabel("firefox", { history: 1, bookmarks: 5 })).toBe(
      "Mozilla Firefox: добавлено 1 страница истории и 5 закладок.",
    );
    expect(bookmarkLabel({ id: id(1), title: "", url: "https://a.example/x" })).toBe("a.example");
    expect(bookmarkLabel({ id: id(1), title: "Docs", url: "https://a.example/" })).toBe("Docs");
  });
});
