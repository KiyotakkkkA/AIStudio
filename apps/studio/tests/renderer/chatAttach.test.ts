import { afterEach, expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  ChatAttachDto,
  ConversationDetailDto,
  QuestionRequestDto,
  RunHandleDto,
  contract,
  type Contract,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore, { GENERATION_FAILED } from "../../src/renderer/features/chat/ChatStore.ts";
import { provider } from "./providerFixtures.ts";

const saved = provider();
const handle = RunHandleDto.parse({
  id: "0199ff11-4444-7111-8111-000000000002",
  streamId: "0199ff11-4444-7111-8111-000000000003",
});
const approvalId = "0199ff11-6666-7111-8111-000000000001";
const conversation = ConversationDetailDto.parse({
  id: "0199ff11-4444-7111-8111-000000000001",
  title: "Отчёт",
  providerId: saved.id,
  modelId: saved.models[0]!.externalId,
  createdAt: 1,
  updatedAt: 1,
  messages: [
    {
      id: "0199ff11-7777-7111-8111-000000000001",
      conversationId: "0199ff11-4444-7111-8111-000000000001",
      role: "user",
      content: "Запусти скрипт",
      citations: [],
      tokensIn: 0,
      tokensOut: 0,
      usageEstimated: true,
      durationMs: 0,
      runId: null,
      partial: false,
      createdAt: 1,
    },
    {
      id: "0199ff11-7777-7111-8111-000000000002",
      conversationId: "0199ff11-4444-7111-8111-000000000001",
      role: "assistant",
      content: "",
      citations: [],
      tokensIn: 0,
      tokensOut: 0,
      usageEstimated: true,
      durationMs: 0,
      runId: handle.id,
      partial: true,
      createdAt: 2,
    },
  ],
});
const question = QuestionRequestDto.parse({
  id: "0199ff11-5555-7111-8111-000000000001",
  runId: handle.id,
  questions: [{ id: "q1", title: "Название", kind: "text", options: [], allowOther: false }],
});

function live(overrides: Partial<ChatAttachDto> = {}): ChatAttachDto {
  return ChatAttachDto.parse({
    run: handle,
    seq: 4,
    startedAt: 1_700_000_000_000,
    content: "Сначала ",
    reasoning: "",
    toolCalls: [],
    approvals: [],
    questions: [],
    ...overrides,
  });
}

function setup(attached: ChatAttachDto | null) {
  let current = attached;
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [conversation])
    .handle("chat.conversations.get", () => conversation)
    .handle("chat.attach", () => current)
    .handle("providers.list", () => [saved])
    .handle("providers.get", () => saved)
    .handle("vectorStores.list", () => [])
    .handle("runs.approve", () => undefined)
    .handle("chat.answer", () => undefined)
    .handle("chat.cancel", () => undefined);
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new ChatStore(createIpcClient(contract, bridge), events);
  return {
    bridge,
    events,
    store,
    attach(value: ChatAttachDto | null) {
      current = value;
    },
    token(seq: number, delta: string) {
      events.dispatch({ type: "token", ...handle, seq, ts: 1, delta });
    },
  };
}

async function reopened(attached: ChatAttachDto | null) {
  const context = setup(attached);
  await context.store.mount();
  await context.store.select(conversation.id);
  return context;
}

afterEach(() => vi.useRealTimers());

test("a reopened chat restores the live turn and continues its text from the next event", async () => {
  vi.useFakeTimers();
  const { bridge, store, token } = await reopened(live());

  expect(bridge.calls).toContainEqual({
    channel: "chat.attach",
    payload: { conversationId: conversation.id },
  });
  expect(store.run).toEqual(handle);
  expect(store.generating).toBe(true);
  expect(store.runStartedAt).toBe(1_700_000_000_000);
  expect(store.liveText).toBe("Сначала ");
  expect(store.active?.messages.map((message) => message.role)).toEqual(["user"]);

  token(4, "повтор");
  token(5, "продолжение");
  vi.advanceTimersByTime(32);
  expect(store.liveText).toBe("Сначала продолжение");
  store.dispose();
});

test("events the host sent before the chat was reopened are not applied twice", async () => {
  vi.useFakeTimers();
  const context = setup(live());
  context.token(4, "старое");
  context.token(5, "новое");
  await context.store.mount();
  await context.store.select(conversation.id);
  vi.advanceTimersByTime(32);
  expect(context.store.liveText).toBe("Сначала новое");
  context.store.dispose();
});

test("a pending approval is shown after reopening and approving it goes to the live run", async () => {
  const { bridge, store } = await reopened(
    live({
      approvals: [
        {
          id: approvalId,
          runId: handle.id,
          subject: "code.run",
          scope: "global",
          expiresAt: 1,
        },
      ],
      toolCalls: [
        {
          id: "call_1",
          key: "code.run",
          name: "code__run",
          round: 0,
          arguments: { language: "python", source: "print(1)" },
          status: "awaiting-approval",
          result: null,
          truncated: false,
          error: null,
          approvalId,
          stepId: null,
          durationMs: 0,
          meta: null,
        },
      ],
    }),
  );

  expect(store.approvalFor(approvalId)?.subject).toBe("code.run");
  expect(store.liveToolCalls[0]?.status).toBe("awaiting-approval");
  await store.decideApproval(approvalId, true);
  expect(bridge.calls.at(-1)).toEqual({
    channel: "runs.approve",
    payload: { id: handle.id, approvalId, always: false },
  });
  expect(store.approvals).toEqual([]);
  store.dispose();
});

test("a pending question is shown after reopening and can be answered", async () => {
  const { bridge, store } = await reopened(live({ questions: [question] }));

  expect(store.questionFor(question.id)?.request).toEqual(question);
  const answers = [{ questionId: "q1", selected: [], text: "Итоги" }];
  await store.answerQuestion(question.id, answers);
  expect(bridge.calls.at(-1)).toEqual({
    channel: "chat.answer",
    payload: { id: handle.id, requestId: question.id, answers },
  });
  expect(store.questions).toEqual([]);
  store.dispose();
});

test("a gap in the stream attaches again and drains the events held meanwhile", async () => {
  vi.useFakeTimers();
  const { attach, bridge, store, token } = await reopened(live());
  attach(live({ seq: 8, content: "Сначала потом " }));

  token(9, "дальше");
  token(10, " и ещё");
  await vi.waitFor(() => expect(store.liveText).toBe("Сначала потом дальше и ещё"));

  expect(bridge.calls.filter((call) => call.channel === "chat.attach")).toHaveLength(2);
  store.dispose();
});

test("a turn that ended during a gap is closed and the conversation reloads", async () => {
  const { attach, bridge, store, token } = await reopened(live());
  attach(null);

  token(9, "дальше");
  await vi.waitFor(() => expect(store.run).toBeNull());

  expect(store.generating).toBe(false);
  await vi.waitFor(() =>
    expect(bridge.calls.filter((call) => call.channel === "chat.conversations.get")).toHaveLength(
      2,
    ),
  );
  store.dispose();
});

test("no live turn leaves the chat idle and a failed run without a message shows the constant", async () => {
  const { store } = await reopened(null);
  expect(store.run).toBeNull();
  expect(store.generating).toBe(false);

  const { events, store: running } = await reopened(live());
  events.dispatch({ type: "end", ...handle, seq: 5, ts: 1, outcome: { status: "failed" } });
  expect(running.error).toBe(GENERATION_FAILED);
  store.dispose();
  running.dispose();
});
