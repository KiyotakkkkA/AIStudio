import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppError,
  AppErrorCode,
  contract,
  ConversationDetailDto,
  ConversationDto,
  MessageDto,
  type Contract,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import { provider } from "./providerFixtures.ts";

const id = "0199bb11-5555-7111-8111-000000000001";
const branchId = "0199bb11-5555-7111-8111-000000000002";
const answerId = "0199bb11-5555-7111-8111-000000000010";
const saved = provider();
const answer = MessageDto.parse({
  id: answerId,
  conversationId: id,
  role: "assistant",
  content: "Ответ",
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
const summary = ConversationDto.parse(conversation);
const branch = ConversationDto.parse({
  ...summary,
  id: branchId,
  title: "Диалог (ветка)",
  updatedAt: 3,
});

function setup(fail = false) {
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [summary])
    .handle("chat.conversations.get", (input) =>
      input.id === branchId
        ? ConversationDetailDto.parse({
            ...branch,
            messages: [{ ...answer, conversationId: branchId }],
          })
        : conversation,
    )
    .handle("providers.list", () => [saved])
    .handle("providers.get", () => saved)
    .handle("vectorStores.list", () => [])
    .handle("chat.conversations.branch", () => {
      if (fail) throw new AppError(AppErrorCode.CONFLICT, "busy");
      return branch;
    });
  const store = new ChatStore(
    createIpcClient(contract, bridge),
    createEventRouter({ logger: { log: vi.fn() } }),
  );
  return { store, bridge };
}

test("branching from an answer opens the new dialog at the top of the list", async () => {
  const { store, bridge } = setup();
  await store.mount();
  await store.select(conversation.id);
  await store.branchConversation(answer.id);
  expect(
    bridge.calls.find((call) => call.channel === "chat.conversations.branch")?.payload,
  ).toEqual({ id, messageId: answer.id });
  expect(store.conversations.map((item) => item.id)).toEqual([branchId, id]);
  expect(store.active?.id).toBe(branchId);
  expect(store.active?.messages).toHaveLength(1);
  expect(store.branching).toBe(false);
});

test("a refused branch keeps the open dialog and shows the error", async () => {
  const { store } = setup(true);
  await store.mount();
  await store.select(conversation.id);
  await store.branchConversation(answer.id);
  expect(store.active?.id).toBe(id);
  expect(store.conversations).toHaveLength(1);
  expect(store.error).not.toBeNull();
  expect(store.branching).toBe(false);
});
