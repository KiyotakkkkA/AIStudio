import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { contract, ConversationDetailDto, MessageDto, type Contract } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import {
  EMPTY_PIN,
  currentPin,
  pinPreview,
} from "../../src/renderer/features/chat/pinnedMessages.ts";
import { provider } from "./providerFixtures.ts";

const id = "0199bb11-4444-7111-8111-000000000001";
const answerId = "0199bb11-4444-7111-8111-000000000010";
const saved = provider();
const answer = MessageDto.parse({
  id: answerId,
  conversationId: id,
  role: "assistant",
  content: "## План\n\nТекст",
  citations: [],
  tokensIn: 1,
  tokensOut: 1,
  usageEstimated: false,
  durationMs: 1,
  runId: null,
  partial: false,
  createdAt: 2,
});
const conversation = ConversationDetailDto.parse({
  id,
  title: "Диалог",
  providerId: saved.id,
  modelId: saved.models[0]!.externalId,
  createdAt: 1,
  updatedAt: 1,
  messages: [answer],
});

function setup() {
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [conversation])
    .handle("chat.conversations.get", () => conversation)
    .handle("providers.list", () => [saved])
    .handle("providers.get", () => saved)
    .handle("vectorStores.list", () => [])
    .handle("chat.messages.pin", (input) => ({ ...answer, pinned: input.pinned }))
    .handle("chat.conversations.pin", (input) => ({ ...conversation, pinned: input.pinned }));
  const store = new ChatStore(
    createIpcClient(contract, bridge),
    createEventRouter({ logger: { log: vi.fn() } }),
  );
  return { store, bridge };
}

test("the preview is the first line of prose without markdown", () => {
  expect(pinPreview("## Заголовок **важно**\n\nТекст")).toBe("Заголовок важно");
  expect(pinPreview("\n> - [Ссылка](https://example.com) и `код` [[1]]")).toBe("Ссылка и код");
  expect(pinPreview("```ts\nconst a = 1;\n```\n\n1. Итог")).toBe("Итог");
  expect(pinPreview("```sql\nSELECT 1;\n```")).toBe("SELECT 1;");
  expect(pinPreview("  \n")).toBe(EMPTY_PIN);
});

test("the current pin is the last one above the threshold, else the first", () => {
  const tops = [
    { id: "a", top: -400 },
    { id: "b", top: 120 },
    { id: "c", top: 900 },
  ];
  expect(currentPin(tops, 200)).toBe("b");
  expect(currentPin(tops, 100)).toBe("a");
  expect(currentPin([{ id: "a", top: 500 }], 100)).toBe("a");
  expect(currentPin([], 100)).toBeNull();
});

test("pinning an answer shows the navigator until it is switched off", async () => {
  const { store, bridge } = setup();
  await store.mount();
  await store.select(conversation.id);
  expect(store.pinnedMessages).toEqual([]);
  expect(store.pinsShown).toBe(false);
  await store.pinMessage(answer.id, true);
  expect(bridge.calls.at(-1)?.payload).toEqual({ id, messageId: answer.id, pinned: true });
  expect(store.pinnedMessages.map((message) => message.id)).toEqual([answer.id]);
  expect(store.pinsShown).toBe(true);
  store.togglePins();
  expect(store.pinsShown).toBe(false);
  store.togglePins();
  await store.pinMessage(answer.id, false);
  expect(store.pinsShown).toBe(false);
});

test("pinning a conversation updates the list and the open dialog", async () => {
  const { store } = setup();
  await store.mount();
  await store.select(conversation.id);
  await store.pinConversation(conversation.id, true);
  expect(store.conversations[0]?.pinned).toBe(true);
  expect(store.active?.pinned).toBe(true);
  expect(store.active?.messages).toHaveLength(1);
});
