import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  ConversationDetailDto,
  ConversationId,
  contract,
  type Contract,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import AppSettingsStore from "../../src/renderer/features/preferences/AppSettingsStore.ts";
import { provider } from "./providerFixtures.ts";

const saved = provider();
const first = ConversationDetailDto.parse({
  id: "0199ee11-5555-7111-8111-000000000001",
  title: "Plans",
  providerId: saved.id,
  modelId: saved.models[0]!.externalId,
  createdAt: 1,
  updatedAt: 1,
  messages: [],
});
const second = ConversationDetailDto.parse({
  ...first,
  id: "0199ee11-5555-7111-8111-000000000002",
});

function setup() {
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [first, second])
    .handle("chat.conversations.get", ({ id }) => (id === second.id ? second : first))
    .handle("providers.list", () => [saved])
    .handle("providers.get", () => saved)
    .handle("vectorStores.list", () => [])
    .handle("notifications.clear", () => undefined);
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new ChatStore(createIpcClient(contract, bridge), events);
  return { bridge, store };
}

function cleared(bridge: ReturnType<typeof setup>["bridge"]) {
  return bridge.calls
    .filter((call) => call.channel === "notifications.clear")
    .map((call) => call.payload);
}

test("a conversation opened before the chat page mounts is selected once it has loaded", async () => {
  const { bridge, store } = setup();
  await store.open(second.id);
  expect(store.active).toBeNull();
  await store.mount();
  expect(store.active?.id).toBe(second.id);
  expect(cleared(bridge)).toEqual([{ kind: "conversation", conversationId: second.id }]);
  store.dispose();
});

test("opening on a mounted page selects the conversation and reveals the latest message", async () => {
  const { bridge, store } = setup();
  await store.mount();
  await store.select(first.id);
  const revealed = store.revealed;
  await store.open(second.id);
  expect(store.active?.id).toBe(second.id);
  expect(store.revealed).toBe(revealed + 1);
  await store.open(second.id);
  expect(store.revealed).toBe(revealed + 2);
  expect(cleared(bridge).at(-1)).toEqual({ kind: "conversation", conversationId: second.id });
  store.dispose();
});

test("a failed clear never blocks selecting the conversation", async () => {
  const { bridge, store } = setup();
  bridge.fail("notifications.clear", AppErrorCode.NATIVE_ERROR, "x");
  await store.mount();
  await store.select(first.id);
  expect(store.active?.id).toBe(first.id);
  expect(store.error).toBeNull();
  store.dispose();
});

test("a link to a conversation that no longer exists opens the chat with no selection", async () => {
  const { bridge, store } = setup();
  const missing = ConversationId.parse("0199ee11-5555-7111-8111-0000000000ff");
  await store.open(missing);
  await store.mount();
  expect(store.active).toBeNull();
  expect(store.error).toBeNull();
  await store.select(first.id);
  await store.open(missing);
  expect(store.active).toBeNull();
  expect(store.error).toBeNull();
  const fetched = bridge.calls
    .filter((call) => call.channel === "chat.conversations.get")
    .map((call) => call.payload);
  expect(fetched).not.toContainEqual({ id: missing });
  store.dispose();
});

test("the pending target from a cold start is taken once and a failure reads as none", async () => {
  const bridge = createFakeBridge<Contract>().handle("notifications.pending", () => ({
    kind: "downloads" as const,
  }));
  const settings = new AppSettingsStore(createIpcClient(contract, bridge));
  expect(await settings.takePendingTarget()).toEqual({ kind: "downloads" });
  bridge.fail("notifications.pending", AppErrorCode.NATIVE_ERROR, "x");
  expect(await settings.takePendingTarget()).toBeNull();
});
