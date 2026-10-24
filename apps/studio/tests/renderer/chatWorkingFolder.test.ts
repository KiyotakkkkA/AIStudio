import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  ConversationDetailDto,
  RunHandleDto,
  contract,
  type Contract,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import {
  FOLDER_ERROR_FALLBACK,
  folderName,
} from "../../src/renderer/features/chat/workingFolderCopy.ts";
import { provider } from "./providerFixtures.ts";

const saved = provider();
const handle = RunHandleDto.parse({
  id: "0199ee11-4444-7111-8111-000000000002",
  streamId: "0199ee11-4444-7111-8111-000000000003",
});
const conversation = ConversationDetailDto.parse({
  id: "0199ee11-4444-7111-8111-000000000001",
  title: "Plans",
  providerId: saved.id,
  modelId: saved.models[0]!.externalId,
  createdAt: 1,
  updatedAt: 1,
  messages: [],
});
const FOLDER = "D:\\work\\site";

function setup() {
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [conversation])
    .handle("chat.conversations.get", () => conversation)
    .handle("chat.conversations.create", () => conversation)
    .handle("providers.list", () => [saved])
    .handle("providers.get", () => saved)
    .handle("vectorStores.list", () => [])
    .handle("chat.send", () => handle)
    .handle("chat.pickFolder", () => ({ path: FOLDER }));
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new ChatStore(createIpcClient(contract, bridge), events);
  return { bridge, events, store };
}

const bound = (access: "granted" | "missing") => ({
  status: "bound" as const,
  conversation: { ...conversation, workingFolder: FOLDER, workingFolderAccess: access },
});

test("an uncovered folder asks for a grant and binds with the chosen scope", async () => {
  const { bridge, store } = setup();
  await store.mount();
  await store.select(conversation.id);
  bridge.handle("chat.bindFolder", (input) =>
    input.grant === undefined ? { status: "needsGrant", path: FOLDER } : bound("granted"),
  );
  await store.pickFolder();
  expect(store.folderPrompt).toEqual({ path: FOLDER });
  expect(store.workingFolder).toBeNull();
  await store.confirmFolder("always");
  expect(bridge.calls.filter((call) => call.channel === "chat.bindFolder").at(-1)?.payload).toEqual(
    { id: conversation.id, path: FOLDER, grant: "always" },
  );
  expect(store.folderPrompt).toBeNull();
  expect(store.workingFolder).toBe(FOLDER);
  expect(store.workingFolderAccess).toBe("granted");
  store.dispose();
});

test("a new chat is created before its first folder is bound", async () => {
  const { bridge, store } = setup();
  await store.mount();
  bridge.handle("chat.bindFolder", () => bound("granted"));
  await store.pickFolder();
  const channels = bridge.calls.map((call) => call.channel);
  expect(channels.indexOf("chat.conversations.create")).toBeLessThan(
    channels.indexOf("chat.bindFolder"),
  );
  expect(store.active?.workingFolder).toBe(FOLDER);
  store.dispose();
});

test("a refused folder is reported by code and removal unbinds", async () => {
  const { bridge, store } = setup();
  await store.mount();
  await store.select(conversation.id);
  bridge.fail("chat.bindFolder", AppErrorCode.PERMISSION_DENIED, "host text");
  await store.pickFolder();
  expect(store.folderError).not.toBeNull();
  expect(store.folderError).not.toBe("host text");
  expect(store.folderError).not.toBe(FOLDER_ERROR_FALLBACK);
  bridge.handle("chat.bindFolder", () => bound("missing"));
  await store.pickFolder();
  expect(store.workingFolderAccess).toBe("missing");
  bridge.handle("chat.unbindFolder", () => conversation);
  await store.unbindFolder();
  expect(store.workingFolder).toBeNull();
  store.dispose();
});

test("folder requests open the access modal and the answer goes to folders.answer", async () => {
  const { bridge, events, store } = setup();
  bridge.handle("folders.answer", () => undefined);
  await store.mount();
  await store.select(conversation.id);
  store.composer.setText("Write it");
  await store.send();
  const request = {
    id: "0199ee11-5555-7111-8111-000000000001",
    runId: handle.id,
    kind: "folder",
    subject: "folder.access",
    scope: "global",
    expiresAt: 60_000,
    conversationId: conversation.id,
    tool: "files.write",
    action: "Запись файла",
    folders: [{ path: FOLDER, creates: ["out"] }],
  };
  events.dispatch({ type: "approval", ...handle, seq: 1, ts: 1, request });
  expect(store.folderRequest?.folders).toEqual(request.folders);
  expect(store.standaloneApprovals).toEqual([]);
  await store.answerFolder("conversation");
  expect(bridge.calls.at(-1)).toEqual({
    channel: "folders.answer",
    payload: { id: handle.id, approvalId: request.id, answer: "conversation" },
  });
  expect(store.folderRequest).toBeNull();
  store.dispose();
});

test("the folder name is the last path segment", () => {
  expect(folderName("D:\\work\\site\\")).toBe("site");
  expect(folderName("/home/me/notes")).toBe("notes");
});

test("the active panel button hides the panel and the other one switches it", async () => {
  const { bridge, store } = setup();
  await store.mount();
  bridge.handle("chat.conversations.get", () => bound("granted").conversation);
  await store.select(conversation.id);

  expect(store.isPanelShown("context")).toBe(true);
  store.showPanel("context");
  expect(store.isPanelShown("context")).toBe(false);
  store.showPanel("files");
  expect(store.isPanelShown("files")).toBe(true);
  expect(store.isPanelShown("context")).toBe(false);
  store.showPanel("context");
  expect(store.isPanelShown("context")).toBe(true);
  expect(store.isPanelShown("files")).toBe(false);
  store.showPanel("context");
  expect(store.panelOpen).toBe(false);
});

test("the conversation list hides and shows again", () => {
  const { store } = setup();
  expect(store.listOpen).toBe(true);
  store.toggleList();
  expect(store.listOpen).toBe(false);
  store.toggleList();
  expect(store.listOpen).toBe(true);
});
