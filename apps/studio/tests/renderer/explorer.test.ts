import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppError,
  AppErrorCode,
  ConversationDetailDto,
  ConversationId,
  ExplorerEntryDto,
  StreamId,
  contract,
  type Contract,
  type ExplorerChangeDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import ExplorerStore from "../../src/renderer/features/chat/explorer/ExplorerStore.ts";
import { provider } from "./providerFixtures.ts";

const CONVERSATION = ConversationId.parse("0199ee11-5555-7111-8111-000000000001");
const OTHER = ConversationId.parse("0199ee11-5555-7111-8111-000000000009");
const STREAM = StreamId.parse("0199ee11-5555-7111-8111-000000000002");

const entry = (path: string, kind: "file" | "folder" = "file") =>
  ExplorerEntryDto.parse({
    name: path.split("/").pop(),
    path,
    kind,
    size: kind === "file" ? 10 : 0,
    modifiedAt: 1,
    ignored: false,
  });

const tree: Record<string, ExplorerEntryDto[]> = {
  "": [entry("docs", "folder"), entry("src", "folder"), entry("README.md")],
  docs: [entry("docs/PLAN.md")],
  src: [entry("src/main.rs")],
};

function setup() {
  const listing = { ...tree };
  const texts: Record<string, string> = { "README.md": "# Readme", "docs/PLAN.md": "# Plan" };
  const bridge = createFakeBridge<Contract>()
    .handle("workspace.list", ({ path }) => ({
      path,
      entries: listing[path] ?? [],
      truncated: false,
    }))
    .handle("workspace.read", ({ path }) => {
      const text = texts[path];
      if (text === undefined) throw new AppError(AppErrorCode.NOT_FOUND, "Файл не найден");
      return { path, size: text.length, modifiedAt: 2, text, truncated: false };
    })
    .handle("workspace.watch", () => ({ streamId: STREAM }))
    .handle("workspace.unwatch", () => undefined);
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new ExplorerStore(createIpcClient(contract, bridge), events);
  let seq = 0;
  const emit = (changes: ExplorerChangeDto[], conversationId = CONVERSATION) => {
    events.dispatch({
      streamId: STREAM,
      seq: seq++,
      ts: 1,
      type: "step",
      step: { domain: "workspace", conversationId, changes },
    });
  };
  return { bridge, store, emit, listing, texts };
}

const calls = (bridge: ReturnType<typeof setup>["bridge"], channel: string) =>
  bridge.calls.filter((call) => call.channel === channel).map((call) => call.payload);

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

test("the root loads on show and folders load once when first expanded", async () => {
  const { bridge, store } = setup();
  await store.show(CONVERSATION);
  expect(store.rows.map((row) => row.entry.path)).toEqual(["docs", "src", "README.md"]);
  expect(calls(bridge, "workspace.watch")).toEqual([{ id: CONVERSATION }]);

  await store.toggle("docs");
  expect(store.rows.map((row) => [row.entry.path, row.depth])).toEqual([
    ["docs", 0],
    ["docs/PLAN.md", 1],
    ["src", 0],
    ["README.md", 0],
  ]);
  await store.toggle("docs");
  await store.toggle("docs");
  expect(calls(bridge, "workspace.list")).toEqual([
    { id: CONVERSATION, path: "" },
    { id: CONVERSATION, path: "docs" },
  ]);
  store.collapseAll();
  expect(store.rows).toHaveLength(3);
});

test("a change reloads only loaded parents and marks the path and its folders", async () => {
  const { bridge, store, emit, listing } = setup();
  await store.show(CONVERSATION);
  await store.toggle("docs");
  const before = calls(bridge, "workspace.list").length;
  listing["docs"] = [entry("docs/NEW.md"), entry("docs/PLAN.md")];

  emit([
    { path: "docs/NEW.md", kind: "created" },
    { path: "src/main.rs", kind: "modified" },
  ]);
  await settle();
  expect(calls(bridge, "workspace.list").slice(before)).toEqual([
    { id: CONVERSATION, path: "docs" },
  ]);
  expect(store.rows.map((row) => row.entry.path)).toContain("docs/NEW.md");
  expect(store.marks.get("docs/NEW.md")).toBe("created");
  expect(store.marks.get("src/main.rs")).toBe("modified");
  expect(store.markOf(entry("docs", "folder"))).toBe("inside");
  expect(store.markOf(entry("src", "folder"))).toBe("inside");
  expect(store.markOf(entry("README.md"))).toBeNull();

  emit([{ path: "docs/NEW.md", kind: "modified" }]);
  await settle();
  expect(store.marks.get("docs/NEW.md")).toBe("created");

  await store.open("docs/NEW.md");
  expect(store.marks.has("docs/NEW.md")).toBe(false);
  store.clearMarks();
  expect(store.hasMarks).toBe(false);
});

test("changes for another conversation are ignored", async () => {
  const { store, emit } = setup();
  await store.show(CONVERSATION);
  emit([{ path: "README.md", kind: "modified" }], OTHER);
  await settle();
  expect(store.hasMarks).toBe(false);
});

test("the open file reloads when it changes and shows the removed state", async () => {
  const { bridge, store, emit, texts } = setup();
  await store.show(CONVERSATION);
  await store.open("README.md");
  expect(store.file?.text).toBe("# Readme");

  texts["README.md"] = "# Readme v2";
  emit([{ path: "README.md", kind: "modified" }]);
  await settle();
  await settle();
  expect(store.file?.text).toBe("# Readme v2");
  expect(store.marks.has("README.md")).toBe(false);
  expect(calls(bridge, "workspace.read")).toHaveLength(2);

  emit([{ path: "README.md", kind: "removed" }]);
  await settle();
  expect(store.fileRemoved).toBe(true);
  expect(store.file?.text).toBe("# Readme v2");

  emit([{ path: "README.md", kind: "created" }]);
  await settle();
  await settle();
  expect(store.fileRemoved).toBe(false);
});

test("removing a folder forgets its children and the root reports the folder gone", async () => {
  const { store, emit } = setup();
  await store.show(CONVERSATION);
  await store.toggle("docs");
  emit([{ path: "docs/PLAN.md", kind: "modified" }]);
  await settle();
  emit([{ path: "docs", kind: "removed" }]);
  await settle();
  expect(store.expanded.has("docs")).toBe(false);
  expect(store.folders.has("docs")).toBe(false);
  expect(store.hasMarks).toBe(false);

  emit([{ path: "", kind: "removed" }]);
  await settle();
  expect(store.root).toBe("gone");
});

test("a missing grant shows the access state and does not watch", async () => {
  const { bridge, store } = setup();
  bridge.fail("workspace.list", AppErrorCode.PERMISSION_DENIED, "Доступ к папке не предоставлен");
  await store.show(CONVERSATION);
  expect(store.root).toBe("missing");
  expect(calls(bridge, "workspace.watch")).toEqual([]);
});

test("hiding unsubscribes, unwatches and drops late events", async () => {
  const { bridge, store, emit } = setup();
  await store.show(CONVERSATION);
  await store.open("README.md");
  store.hide();
  expect(calls(bridge, "workspace.unwatch")).toEqual([{ id: CONVERSATION }]);
  expect(store.conversationId).toBeNull();
  expect(store.fileOpen).toBe(false);
  emit([{ path: "README.md", kind: "modified" }]);
  await settle();
  expect(store.hasMarks).toBe(false);
  expect(calls(bridge, "workspace.list")).toHaveLength(1);
});

test("showing another conversation switches the watch", async () => {
  const { bridge, store } = setup();
  await store.show(CONVERSATION);
  await store.show(CONVERSATION);
  expect(calls(bridge, "workspace.watch")).toHaveLength(1);
  await store.show(OTHER);
  expect(calls(bridge, "workspace.unwatch")).toEqual([{ id: CONVERSATION }]);
  expect(calls(bridge, "workspace.watch")).toEqual([{ id: CONVERSATION }, { id: OTHER }]);
  store.dispose();
});

test("arrow keys move focus through visible rows and expand folders", async () => {
  const { store } = setup();
  await store.show(CONVERSATION);
  store.moveFocus(1);
  expect(store.focused).toBe("docs");
  await store.expandFocused();
  expect(store.expanded.has("docs")).toBe(true);
  await store.expandFocused();
  expect(store.focused).toBe("docs/PLAN.md");
  store.collapseFocused();
  expect(store.focused).toBe("docs");
  store.collapseFocused();
  expect(store.expanded.has("docs")).toBe(false);
  store.moveFocus(-1);
  expect(store.focused).toBe("docs");
});

test("the side panel falls back to the run context without a working folder", async () => {
  const saved = provider();
  const conversation = ConversationDetailDto.parse({
    id: CONVERSATION,
    title: "Files",
    providerId: saved.id,
    modelId: saved.models[0]!.externalId,
    createdAt: 1,
    updatedAt: 1,
    messages: [],
  });
  const bound = { ...conversation, workingFolder: "D:\\work", workingFolderAccess: "granted" };
  let current: ConversationDetailDto = conversation;
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [current])
    .handle("chat.conversations.get", () => current)
    .handle("providers.list", () => [saved])
    .handle("providers.get", () => saved)
    .handle("vectorStores.list", () => []);
  const chat = new ChatStore(
    createIpcClient(contract, bridge),
    createEventRouter({ logger: { log: vi.fn() } }),
  );
  await chat.mount();
  await chat.select(conversation.id);

  chat.showPanel("files");
  expect(chat.activePanel).toBe("context");

  current = ConversationDetailDto.parse(bound);
  await chat.select(conversation.id);
  expect(chat.activePanel).toBe("files");
  expect(chat.panelOpen).toBe(true);
  chat.showPanel("files");
  expect(chat.panelOpen).toBe(false);
  chat.showPanel("context");
  expect(chat.activePanel).toBe("context");
  expect(chat.panelOpen).toBe(true);
  chat.closePanel();
  expect(chat.panelOpen).toBe(false);
  chat.dispose();
});
