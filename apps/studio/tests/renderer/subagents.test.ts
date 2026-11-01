import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppError,
  AppErrorCode,
  ChatToolCallDto,
  ConversationDetailDto,
  RunHandleDto,
  contract,
  type Contract,
  type ProviderDto,
  type SubagentPreferencesDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import SubagentSteps, {
  AGENT_DELEGATE,
  delegateRuns,
  subagentMeta,
} from "../../src/renderer/features/chat/SubagentSteps.ts";
import SubagentPreferencesStore from "../../src/renderer/features/subagents/SubagentPreferencesStore.ts";
import {
  SUBAGENT_SAVE_FAILED,
  subagentApprovalTitle,
  subagentDuration,
  subagentGroupTitle,
  subagentTokens,
  subagentTools,
  SUBAGENT_APPROVAL_PREFIX,
} from "../../src/renderer/features/subagents/subagentCopy.ts";
import { MODEL, OLLAMA, OTHER_MODEL, model, provider, summary } from "./providerFixtures.ts";

const ACCOUNT = "0199bb11-1111-7111-8111-000000000009";
const CHILD = "0199cc11-1111-7111-8111-000000000020";
const OTHER_CHILD = "0199cc11-1111-7111-8111-000000000021";

function preferencesSetup(
  providers: ProviderDto[] = [provider()],
  initial: SubagentPreferencesDto = { enabled: true, models: [], concurrency: 2 },
) {
  let stored = initial;
  const saved: SubagentPreferencesDto[] = [];
  let refusal: Error | null = null;
  const bridge = createFakeBridge<Contract>()
    .handle("providers.list", () => providers.map((item) => summary({ id: item.id })))
    .handle("providers.get", ({ id }) => providers.find((item) => item.id === id) ?? provider())
    .handle("subagents.get", () => stored)
    .handle("subagents.set", (value) => {
      if (refusal !== null) throw refusal;
      saved.push(value);
      stored = value;
      return value;
    });
  const store = new SubagentPreferencesStore(createIpcClient(contract, bridge));
  return {
    store,
    saved,
    refuse(error: Error) {
      refusal = error;
    },
  };
}

test("the preferences load, add a model through the provider picker and save the ordered list", async () => {
  const { store, saved } = preferencesSetup([
    provider({
      models: [
        model({ id: MODEL, externalId: "gone", available: false }),
        model({ id: OTHER_MODEL, externalId: "qwen3:8b" }),
      ],
    }),
  ]);
  await store.load();
  expect(store.loaded).toBe(true);
  expect(store.canAdd).toBe(false);
  store.selectDraftProvider(OLLAMA);
  expect(store.draftModelId).toBe("qwen3:8b");
  expect(store.canAdd).toBe(true);
  await store.addDraft();
  expect(saved.at(-1)).toEqual({
    enabled: true,
    models: [{ providerId: OLLAMA, modelId: "qwen3:8b" }],
    concurrency: 2,
  });
  store.selectDraftProvider(OLLAMA);
  expect(store.canAdd).toBe(false);
});

test("entries move, are removed, and concurrency stays between one and four", async () => {
  const first = { providerId: OLLAMA, modelId: "a" };
  const second = { providerId: OLLAMA, modelId: "b" };
  const { store, saved } = preferencesSetup([provider()], {
    enabled: true,
    models: [first, second],
    concurrency: 2,
  });
  await store.load();
  await store.move(1, -1);
  expect(store.models).toEqual([second, first]);
  await store.move(0, -1);
  expect(saved).toHaveLength(1);
  await store.remove(0);
  expect(store.models).toEqual([first]);
  await store.setConcurrency(9);
  expect(store.concurrency).toBe(4);
  await store.setConcurrency(0);
  expect(store.concurrency).toBe(1);
  await store.setEnabled(false);
  expect(saved.at(-1)).toEqual({ enabled: false, models: [first], concurrency: 1 });
});

test("an account-mode model shows the serialisation note", async () => {
  const account = provider({
    id: ACCOUNT as never,
    authMode: "account",
    models: [model({ id: OTHER_MODEL, providerId: ACCOUNT as never, externalId: "web" })],
  });
  const { store } = preferencesSetup([provider(), account], {
    enabled: true,
    models: [{ providerId: OLLAMA, modelId: "missing" }],
    concurrency: 3,
  });
  await store.load();
  expect(store.accountListed).toBe(false);
  expect(store.views[0]?.missing).toBe(true);
  store.selectDraftProvider(ACCOUNT);
  await store.addDraft();
  expect(store.accountListed).toBe(true);
});

test("a refused save shows the host reason and restores the stored list", async () => {
  const { store, refuse } = preferencesSetup();
  await store.load();
  refuse(new AppError(AppErrorCode.VALIDATION_FAILED, "Модель субагента недоступна"));
  store.selectDraftProvider(OLLAMA);
  await store.addDraft();
  expect(store.error).toBe("Модель субагента недоступна");
  expect(store.models).toEqual([]);
  refuse(new Error("offline"));
  await store.setEnabled(false);
  expect(store.error).toBe(SUBAGENT_SAVE_FAILED);
  expect(store.enabled).toBe(true);
});

function childCall(index: number, status: ChatToolCallDto["status"], approvalId: string | null) {
  return ChatToolCallDto.parse({
    id: `call_${String(index)}`,
    key: "fs.read",
    name: "fs__read",
    round: 0,
    arguments: { path: "lib.rs" },
    status,
    result: null,
    truncated: false,
    error: null,
    approvalId,
    stepId: null,
    durationMs: 0,
  });
}

function delegate(id: string, runId: string, status: string): ChatToolCallDto {
  return ChatToolCallDto.parse({
    id,
    key: AGENT_DELEGATE,
    name: "delegate_task",
    round: 0,
    arguments: { task: "Найди x", preset: "explore" },
    status: status === "running" ? "running" : "succeeded",
    result: null,
    truncated: false,
    error: null,
    approvalId: null,
    stepId: null,
    durationMs: 0,
    meta: {
      runId,
      preset: "explore",
      label: "Найди x",
      task: "Найди x",
      model: "qwen3:8b",
      modelSource: "preferred",
      status,
      partial: status === "partial",
      toolCalls: 2,
      tokens: 1234,
      report: "Отчёт",
      durationMs: 65_000,
    },
  });
}

test("child steps are grouped by run id and replace by index", () => {
  const steps = new SubagentSteps();
  expect(
    steps.apply({
      domain: "tool-call",
      runId: CHILD,
      index: 0,
      call: childCall(0, "running", null),
    }),
  ).toBe(false);
  expect(
    steps.apply({
      domain: "subagent",
      runId: CHILD,
      index: 0,
      call: childCall(0, "running", null),
    }),
  ).toBe(true);
  steps.apply({
    domain: "subagent",
    runId: CHILD,
    index: 0,
    call: childCall(0, "succeeded", null),
  });
  steps.apply({
    domain: "subagent",
    runId: CHILD,
    index: 1,
    call: childCall(1, "awaiting-approval", "a1"),
  });
  steps.apply({
    domain: "subagent",
    runId: OTHER_CHILD,
    index: 0,
    call: childCall(0, "running", null),
  });
  expect(steps.callsFor(CHILD).map((call) => call.status)).toEqual([
    "succeeded",
    "awaiting-approval",
  ]);
  expect(steps.callsFor(OTHER_CHILD)).toHaveLength(1);
  expect(steps.callsFor(undefined)).toEqual([]);
  expect([...steps.approvalIds]).toEqual(["a1"]);
  steps.clear();
  expect(steps.callsFor(CHILD)).toEqual([]);
});

test("consecutive delegates form one group and the meta is read", () => {
  const lookup = { ...childCall(9, "succeeded", null), key: "demo.lookup" };
  const groups = delegateRuns([
    delegate("d1", CHILD, "running"),
    delegate("d2", OTHER_CHILD, "partial"),
    lookup,
    delegate("d3", CHILD, "ok"),
  ]);
  expect(groups.map((group) => group.map((call) => call.id))).toEqual([
    ["d1", "d2"],
    ["call_9"],
    ["d3"],
  ]);
  const meta = subagentMeta(delegate("d2", OTHER_CHILD, "partial"));
  expect(meta).toMatchObject({
    runId: OTHER_CHILD,
    partial: true,
    tokens: 1234,
    durationMs: 65_000,
  });
  expect(subagentMeta(lookup)).toBeNull();
});

test("the copy uses plural forms and ru-RU numbers", () => {
  expect(subagentTools(1)).toBe(`${(1).toLocaleString("ru-RU")} инструмент`);
  expect(subagentTools(3)).toBe(`${(3).toLocaleString("ru-RU")} инструмента`);
  expect(subagentTools(12)).toBe(`${(12).toLocaleString("ru-RU")} инструментов`);
  expect(subagentTokens(12_345)).toBe(`${(12_345).toLocaleString("ru-RU")} токенов`);
  expect(subagentGroupTitle(2)).toBe(`${(2).toLocaleString("ru-RU")} субагента`);
  expect(subagentDuration(65_000)).toBe(
    `${(1).toLocaleString("ru-RU")} мин ${(5).toLocaleString("ru-RU")} с`,
  );
  expect(subagentApprovalTitle("Найди x")).toBe(`${SUBAGENT_APPROVAL_PREFIX}: Найди x`);
});

const conversationId = "0199bb11-4444-7111-8111-000000000001";
const handle = RunHandleDto.parse({
  id: "0199bb11-4444-7111-8111-000000000002",
  streamId: "0199bb11-4444-7111-8111-000000000003",
});

function chatSetup() {
  const saved = provider();
  const detail = ConversationDetailDto.parse({
    id: conversationId,
    title: "Диалог",
    providerId: saved.id,
    modelId: saved.models[0]!.externalId,
    settings: {},
    systemPrompt: "",
    attachedStoreIds: [],
    createdAt: 1,
    updatedAt: 1,
    messages: [],
  });
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [detail])
    .handle("chat.conversations.get", () => detail)
    .handle("providers.list", () => [saved])
    .handle("providers.get", () => saved)
    .handle("vectorStores.list", () => [])
    .handle("chat.send", () => handle)
    .handle("runs.approve", () => undefined)
    .handle("runs.cancel", () => undefined);
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new ChatStore(createIpcClient(contract, bridge), events);
  return { store, bridge, events };
}

test("a child approval reaches the dialog, is labelled, and is answered on the child run", async () => {
  const { store, bridge, events } = chatSetup();
  await store.mount();
  await store.select(conversationId as never);
  store.composer.setText("Исследуй");
  await store.send();
  const approvalId = "0199bb11-5555-7111-8111-000000000009";
  events.dispatch({
    type: "approval",
    streamId: handle.streamId,
    seq: 1,
    ts: Date.now(),
    request: {
      id: approvalId,
      runId: CHILD,
      subject: "demo.drop",
      conversationId,
      subagent: { runId: CHILD, label: "Удали временные файлы" },
    },
  });
  events.dispatch({
    type: "approval",
    streamId: handle.streamId,
    seq: 2,
    ts: Date.now(),
    request: { id: "0199bb11-5555-7111-8111-000000000010", runId: CHILD, subject: "demo.drop" },
  });
  expect(store.approvals).toHaveLength(1);
  expect(store.standaloneApprovals[0]?.subagent).toEqual({
    runId: CHILD,
    label: "Удали временные файлы",
  });
  events.dispatch({
    type: "step",
    streamId: handle.streamId,
    seq: 3,
    ts: Date.now(),
    step: {
      domain: "subagent",
      runId: CHILD,
      index: 0,
      call: childCall(0, "awaiting-approval", approvalId),
    },
  });
  expect(store.standaloneApprovals).toEqual([]);
  expect(store.approvalFor(approvalId)?.id).toBe(approvalId);
  await store.decideApproval(approvalId, true);
  expect(bridge.calls.at(-1)).toMatchObject({
    channel: "runs.approve",
    payload: { id: CHILD, approvalId, always: false },
  });

  await store.stopSubagent(CHILD);
  expect(bridge.calls.at(-1)).toMatchObject({ channel: "runs.cancel", payload: { id: CHILD } });
  expect(store.stoppingSubagents).toEqual([]);
  store.dispose();
  events.dispose();
});
