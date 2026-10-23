import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { AppErrorCode, ConversationDetailDto, contract, type Contract } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
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
