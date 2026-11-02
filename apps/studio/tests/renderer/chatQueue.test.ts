import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppError,
  AppErrorCode,
  CHAT_QUEUE_MAX,
  QUEUE_STEP_DOMAIN,
  contract,
  type ChatQueueDto,
  type ChatQueuedMessageDto,
  type Contract,
  type ConversationId,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import QueueStore from "../../src/renderer/features/chat/QueueStore.ts";
import { QUEUE_ADD_FAILED, QUEUE_SEND_FAILED } from "../../src/renderer/features/chat/queueCopy.ts";

const DIALOG = "0199aa11-2222-7111-8111-111111111111" as ConversationId;
const OTHER = "0199aa11-2222-7111-8111-222222222222" as ConversationId;

function queued(id: string, text: string): ChatQueuedMessageDto {
  return { id, text, createdAt: 1 };
}

function queue(messages: ChatQueuedMessageDto[], urgent = false): ChatQueueDto {
  return { conversationId: DIALOG, messages, urgent };
}

function setup(conversation: ConversationId | null = DIALOG) {
  let state = queue([]);
  const sent: ConversationId[] = [];
  const bridge = createFakeBridge<Contract>()
    .handle("chat.queue.list", () => state)
    .handle("chat.queue.add", (input) => {
      state = queue([
        ...state.messages,
        queued(`q${String(state.messages.length + 1)}`, input.text),
      ]);
      return state;
    })
    .handle("chat.queue.edit", (input) => {
      state = queue(
        state.messages.map((message) =>
          message.id === input.messageId ? { ...message, text: input.text } : message,
        ),
      );
      return state;
    })
    .handle("chat.queue.remove", (input) => {
      state = queue(state.messages.filter((message) => message.id !== input.messageId));
      return state;
    })
    .handle("chat.queue.flush", () => {
      state = queue(state.messages, true);
      return state;
    });
  const store = new QueueStore(createIpcClient(contract, bridge), {
    conversationId: () => conversation,
    sent: (id) => sent.push(id),
  });
  return { store, bridge, sent, set: (next: ChatQueueDto) => (state = next) };
}

test("messages are added trimmed, edited in place and removed by selection", async () => {
  const { store, bridge } = setup();
  expect(await store.add("  Первое  ")).toBe(true);
  await store.add("Второе");
  expect(bridge.calls.find((call) => call.channel === "chat.queue.add")?.payload).toEqual({
    conversationId: DIALOG,
    text: "Первое",
  });
  expect(store.messages.map((message) => message.text)).toEqual(["Первое", "Второе"]);

  store.move(1);
  expect(store.selected?.text).toBe("Первое");
  store.move(1);
  store.move(1);
  expect(store.selected?.text).toBe("Второе");
  store.startEdit();
  expect(store.draft).toBe("Второе");
  store.setDraft("Исправлено");
  expect(await store.saveEdit()).toBe(true);
  expect(store.editingId).toBeNull();
  expect(store.messages[1]?.text).toBe("Исправлено");

  expect(await store.remove()).toBe(true);
  expect(store.messages.map((message) => message.text)).toEqual(["Первое"]);
  expect(store.selected?.text).toBe("Первое");
});

test("sending now marks the queue urgent and an emptied queue reports the dialog once", async () => {
  const { store, sent } = setup();
  await store.add("Срочно");
  expect(await store.sendNow()).toBe(true);
  expect(store.urgent).toBe(true);

  expect(
    store.apply({ domain: QUEUE_STEP_DOMAIN, conversationId: DIALOG, messages: [], urgent: false }),
  ).toBe(true);
  expect(store.messages).toEqual([]);
  expect(sent).toEqual([DIALOG]);
  store.apply({ domain: QUEUE_STEP_DOMAIN, conversationId: DIALOG, messages: [], urgent: false });
  expect(sent).toEqual([DIALOG]);
});

test("steps of other dialogs and other domains are ignored", () => {
  const { store } = setup();
  expect(
    store.apply({
      domain: QUEUE_STEP_DOMAIN,
      conversationId: OTHER,
      messages: [queued("x", "Чужое")],
      urgent: false,
    }),
  ).toBe(true);
  expect(store.messages).toEqual([]);
  expect(store.apply({ domain: "goal", conversationId: DIALOG })).toBe(false);
});

test("a full queue refuses locally and a failed call shows its copy", async () => {
  const { store, set } = setup();
  set(
    queue(Array.from({ length: CHAT_QUEUE_MAX }, (_, index) => queued(`q${String(index)}`, "x"))),
  );
  await store.load(DIALOG);
  expect(store.full).toBe(true);
  expect(await store.add("Ещё")).toBe(false);

  const failing = new QueueStore(
    createIpcClient(
      contract,
      createFakeBridge<Contract>()
        .handle("chat.queue.add", () => {
          throw new AppError(AppErrorCode.CONFLICT, "full");
        })
        .handle("chat.queue.flush", () => {
          throw new AppError(AppErrorCode.CONFLICT, "busy");
        }),
    ),
    { conversationId: () => DIALOG, sent: () => undefined },
  );
  expect(await failing.add("Текст")).toBe(false);
  expect(failing.error).toBe(QUEUE_ADD_FAILED);
  expect(await failing.sendNow()).toBe(false);
  expect(failing.error).toBe(QUEUE_ADD_FAILED);
  failing.apply({
    domain: QUEUE_STEP_DOMAIN,
    conversationId: DIALOG,
    messages: [queued("q1", "Есть")],
    urgent: false,
  });
  expect(await failing.sendNow()).toBe(false);
  expect(failing.error).toBe(QUEUE_SEND_FAILED);
});

test("without an open dialog nothing is queued", async () => {
  const { store, bridge } = setup(null);
  expect(await store.add("Текст")).toBe(false);
  expect(bridge.calls).toEqual([]);
});
