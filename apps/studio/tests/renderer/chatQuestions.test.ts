import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  ChatToolCallDto,
  ConversationDetailDto,
  QuestionRequestDto,
  RunHandleDto,
  contract,
  type Contract,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import {
  EMPTY_DRAFT,
  NO_ANSWER,
  OTHER_OPTION,
  answerText,
  draftAnswers,
  pick,
  recommendedFirst,
  recordSummary,
  toggleOther,
  writeOther,
} from "../../src/renderer/features/chat/questionCopy.ts";
import { toolCallSummary } from "../../src/renderer/features/chat/toolCallSummary.ts";
import { provider } from "./providerFixtures.ts";

const saved = provider();
const handle = RunHandleDto.parse({
  id: "0199ee11-4444-7111-8111-000000000002",
  streamId: "0199ee11-4444-7111-8111-000000000003",
});
const conversation = ConversationDetailDto.parse({
  id: "0199ee11-4444-7111-8111-000000000001",
  title: "Report",
  providerId: saved.id,
  modelId: saved.models[0]!.externalId,
  createdAt: 1,
  updatedAt: 1,
  messages: [],
});
const request = QuestionRequestDto.parse({
  id: "0199ee11-5555-7111-8111-000000000001",
  runId: handle.id,
  questions: [
    {
      id: "q1",
      title: "Формат",
      kind: "single",
      options: [
        { id: "o1", label: "DOCX", recommended: false },
        { id: "o2", label: "PDF", recommended: true },
      ],
      allowOther: true,
    },
    {
      id: "q2",
      title: "Разделы",
      kind: "multiple",
      options: [
        { id: "o1", label: "Введение", recommended: false },
        { id: "o2", label: "Выводы", recommended: false },
      ],
      allowOther: false,
    },
    { id: "q3", title: "Название", kind: "text", options: [], allowOther: false },
  ],
});
const [format, sections, name] = request.questions as [
  (typeof request.questions)[number],
  (typeof request.questions)[number],
  (typeof request.questions)[number],
];

function setup() {
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [conversation])
    .handle("chat.conversations.get", () => conversation)
    .handle("providers.list", () => [saved])
    .handle("providers.get", () => saved)
    .handle("vectorStores.list", () => [])
    .handle("chat.send", () => handle)
    .handle("chat.answer", () => undefined)
    .handle("chat.skip", () => undefined);
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new ChatStore(createIpcClient(contract, bridge), events);
  return { bridge, events, store };
}

async function asked() {
  const context = setup();
  await context.store.mount();
  await context.store.select(conversation.id);
  context.store.composer.setText("Make a report");
  await context.store.send();
  context.events.dispatch({ type: "question", ...handle, seq: 1, ts: 1, request });
  return context;
}

test("a question event becomes a pending card and the answers go to chat.answer", async () => {
  const { bridge, events, store } = await asked();
  events.dispatch({ type: "question", ...handle, seq: 2, ts: 1, request });
  expect(store.questions).toHaveLength(1);
  expect(store.questionFor(request.id)?.request).toEqual(request);
  const answers = [{ questionId: "q1", selected: ["o2"] }];
  await store.answerQuestion(request.id, answers);
  expect(bridge.calls.at(-1)).toEqual({
    channel: "chat.answer",
    payload: { id: handle.id, requestId: request.id, answers },
  });
  expect(store.questions).toEqual([]);
  store.dispose();
});

test("a skip goes to chat.skip and a failed answer keeps the card with the error", async () => {
  const { bridge, store } = await asked();
  bridge.fail("chat.answer", AppErrorCode.CONFLICT, "Вопрос больше не ожидает ответа");
  await store.answerQuestion(request.id, [{ questionId: "q3", selected: [], text: "Итоги" }]);
  expect(store.questionFor(request.id)?.busy).toBe(false);
  expect(store.error).toBe("Вопрос больше не ожидает ответа");
  await store.skipQuestion(request.id);
  expect(bridge.calls.at(-1)).toEqual({
    channel: "chat.skip",
    payload: { id: handle.id, requestId: request.id },
  });
  expect(store.questions).toEqual([]);
  store.dispose();
});

test("the end of the run drops open questions", async () => {
  const { events, store } = await asked();
  events.dispatch({ type: "end", ...handle, seq: 2, ts: 1, outcome: { status: "cancelled" } });
  expect(store.questions).toEqual([]);
  store.dispose();
});

test("a draft turns picks, other text and typed text into answers in question order", () => {
  let draft = pick(EMPTY_DRAFT, sections, "o2");
  draft = pick(draft, sections, "o1");
  draft = pick(draft, sections, "o2");
  draft = writeOther(draft, name, "  Итоги квартала ");
  draft = pick(draft, format, "o1");
  expect(draftAnswers(request, draft)).toEqual([
    { questionId: "q1", selected: ["o1"] },
    { questionId: "q2", selected: ["o1"] },
    { questionId: "q3", selected: [], text: "Итоги квартала" },
  ]);
  draft = toggleOther(draft, format);
  draft = writeOther(draft, format, "HTML");
  expect(draftAnswers(request, draft)[0]).toEqual({
    questionId: "q1",
    selected: [],
    text: "HTML",
  });
  draft = pick(draft, format, "o2");
  expect(draftAnswers(request, draft)[0]).toEqual({ questionId: "q1", selected: ["o2"] });
  expect(draftAnswers(request, EMPTY_DRAFT)).toEqual([]);
});

test("answers read back as labels, the recommended option leads and summaries count", () => {
  expect(recommendedFirst(format).map((option) => option.id)).toEqual(["o2", "o1"]);
  expect(answerText(format, { questionId: "q1", selected: [], text: "HTML" })).toBe(
    `${OTHER_OPTION}: HTML`,
  );
  expect(answerText(sections, { questionId: "q2", selected: ["o1", "o2"] })).toBe(
    "Введение, Выводы",
  );
  expect(answerText(name, undefined)).toBe(NO_ANSWER);
  const record = { request, answers: [{ questionId: "q1", selected: ["o2"] }], skipped: false };
  const call = ChatToolCallDto.parse({
    id: "call_1",
    key: "chat.ask",
    name: "ask_user",
    round: 0,
    arguments: {},
    status: "succeeded",
    meta: record,
  });
  expect(toolCallSummary(call)).toBe(recordSummary(record, false));
  expect(recordSummary({ ...record, answers: [] }, true)).toBe("3 вопроса");
});
