import { afterEach, expect, test, vi } from "vitest";
import { reaction } from "mobx";
import { createIpcClient } from "@zvs/ipc";
import {
  contract,
  ConversationDetailDto,
  RunHandleDto,
  MessageDto,
  DocumentSelectionDto,
  type Contract,
  type Json,
  VectorStoreId,
  type AccountDto,
  type ProviderId,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore, { SELECTIONS_FULL } from "../../src/renderer/features/chat/ChatStore.ts";
import { shouldPinToBottom } from "../../src/renderer/features/chat/scroll.ts";
import { ADAPTERS, MODEL, model, provider } from "./providerFixtures.ts";

const id = "0199bb11-4444-7111-8111-000000000001";
const handle = RunHandleDto.parse({
  id: "0199bb11-4444-7111-8111-000000000002",
  streamId: "0199bb11-4444-7111-8111-000000000003",
});
const saved = provider();
const conversation = ConversationDetailDto.parse({
  id,
  title: "New conversation",
  providerId: saved.id,
  modelId: saved.models[0]!.externalId,
  settings: {},
  systemPrompt: "",
  attachedStoreIds: [],
  createdAt: Date.now(),
  updatedAt: Date.now(),
  messages: [],
});

function setup() {
  let detail = structuredClone(conversation);
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [detail])
    .handle("chat.conversations.get", () => detail)
    .handle("chat.conversations.create", () => detail)
    .handle("providers.list", () => [saved])
    .handle("providers.get", () => saved)
    .handle("vectorStores.list", () => [])
    .handle("chat.send", () => handle)
    .handle("chat.cancel", () => undefined);
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new ChatStore(createIpcClient(contract, bridge), events);
  return {
    store,
    bridge,
    events,
    setDetail(value: typeof detail) {
      detail = value;
    },
    token(seq: number, delta: string, kind?: string) {
      events.dispatch({ type: "token", ...handle, seq, ts: Date.now(), delta, kind });
    },
    end(status: string, seq = 100) {
      events.dispatch({ type: "end", ...handle, seq, ts: Date.now(), outcome: { status } });
    },
  };
}

afterEach(() => vi.useRealTimers());

test("retrieval cards use real step results including zero hits, and approvals are explicit", async () => {
  const { store, events, bridge, setDetail } = setup();
  const storeId = VectorStoreId.parse("0199bb11-5555-7111-8111-000000000001");
  setDetail({ ...conversation, attachedStoreIds: [storeId] });
  await store.mount();
  await store.select(conversation.id);
  store.composer.setText("Search");
  await store.send();
  events.dispatch({
    type: "step",
    streamId: handle.streamId,
    seq: 1,
    ts: Date.now(),
    step: { nodeId: "store0", status: "succeeded", output: [] },
  });
  expect(store.searchedStoreIds).toEqual([storeId]);
  events.dispatch({
    type: "step",
    streamId: handle.streamId,
    seq: 2,
    ts: Date.now(),
    step: {
      nodeId: "store0",
      status: "succeeded",
      output: [
        {
          id: "hit",
          documentId: "manual",
          chunkIndex: 2,
          path: "manual.md",
          score: 0.81,
          payload: {},
        },
      ],
    },
  });
  expect(store.liveCitations).toMatchObject([{ storeId, sourcePath: "manual.md", score: 0.81 }]);
  const approvalId = "0199bb11-5555-7111-8111-000000000002";
  events.dispatch({
    type: "approval",
    streamId: handle.streamId,
    seq: 3,
    ts: Date.now(),
    request: { id: approvalId, runId: handle.id, subject: "llm.generate" },
  });
  expect(bridge.calls.some((call) => call.channel === "runs.approve")).toBe(false);
  bridge.handle("runs.approve", () => undefined);
  await store.decideApproval(approvalId, true);
  expect(bridge.calls.at(-1)?.payload).toEqual({ id: handle.id, approvalId, always: false });
  expect(store.approvals).toEqual([]);
  store.dispose();
  events.dispose();
});

test("batches tokens, excludes reasoning, and ignores older, duplicate and foreign events", async () => {
  vi.useFakeTimers();
  const { store, token, events } = setup();
  await store.mount();
  store.composer.setText("hello");
  await store.send();
  const changes: string[] = [];
  const dispose = reaction(
    () => store.liveText,
    (text) => changes.push(text),
  );
  token(1, "A");
  token(2, "B");
  token(2, "duplicate");
  token(0, "old");
  token(3, "hidden", "reasoning");
  events.dispatch({
    type: "token",
    streamId: "0199bb11-4444-7111-8111-000000000099",
    seq: 1,
    ts: Date.now(),
    delta: "foreign",
  });
  expect(store.liveText).toBe("");
  vi.advanceTimersByTime(32);
  expect(changes).toEqual(["AB"]);
  dispose();
  store.dispose();
  events.dispose();
});

test("cancel flushes a partial message, reconciles persistence, and ignores events after end", async () => {
  vi.useFakeTimers();
  const { store, token, end, setDetail, bridge, events } = setup();
  await store.mount();
  store.composer.setText("hello");
  await store.send();
  token(1, "Partial answer");
  await store.stop();
  expect(bridge.calls.at(-1)?.channel).toBe("chat.cancel");
  const message = MessageDto.parse({
    id: "0199bb11-4444-7111-8111-000000000004",
    conversationId: id,
    role: "assistant",
    content: "Partial answer",
    citations: [],
    tokensIn: 4,
    tokensOut: 3,
    usageEstimated: true,
    durationMs: 42,
    runId: handle.id,
    partial: true,
    createdAt: Date.now(),
  });
  setDetail({ ...conversation, messages: [message] });
  end("cancelled");
  expect(store.liveText).toBe("Partial answer");
  await vi.waitFor(() => expect(store.active?.messages[0]?.partial).toBe(true));
  expect(store.generating).toBe(false);
  expect(store.outcome).toBe("cancelled");
  token(101, "late");
  vi.advanceTimersByTime(32);
  expect(store.active?.messages[0]?.content).toBe("Partial answer");
  store.dispose();
  events.dispose();
});

test("disposal flushes the buffer and remount replays held events once", async () => {
  vi.useFakeTimers();
  const { store, token, events } = setup();
  await store.mount();
  store.composer.setText("hello");
  await store.send();
  token(1, "A");
  store.dispose();
  expect(vi.getTimerCount()).toBe(0);
  token(2, "B");
  expect(store.liveText).toBe("A");
  await store.mount();
  vi.advanceTimersByTime(32);
  expect(store.liveText).toBe("AB");
  store.dispose();
  events.dispose();
});

test("leaving the page keeps a live turn's approvals, text and start time", async () => {
  const { store, token, events } = setup();
  await store.mount();
  store.composer.setText("hello");
  await store.send();
  const startedAt = store.runStartedAt;
  expect(startedAt).not.toBeNull();
  store.unmount();
  const approvalId = "0199bb11-5555-7111-8111-000000000003";
  for (let seq = 1; seq <= 300; seq += 1) token(seq, "x");
  events.dispatch({
    type: "approval",
    streamId: handle.streamId,
    seq: 301,
    ts: Date.now(),
    request: { id: approvalId, runId: handle.id, subject: "shell.run" },
  });
  await store.mount();
  await vi.waitFor(() => {
    expect(store.liveText).toHaveLength(300);
  });
  expect(store.approvals.map((request) => request.id)).toEqual([approvalId]);
  expect(store.runStartedAt).toBe(startedAt);
  store.dispose();
  events.dispose();
});

test("end replayed synchronously during subscribe does not leak a listener", async () => {
  const { store, bridge, end, events } = setup();
  const subscribe = vi.spyOn(events, "subscribe");
  bridge.handle("chat.send", () => {
    end("failed");
    return handle;
  });
  await store.mount();
  store.composer.setText("hello");
  await store.send();
  expect(store.generating).toBe(false);
  expect(store.outcome).toBe("failed");
  expect(subscribe).toHaveBeenCalledOnce();
  store.dispose();
  events.dispose();
});

test("latest selection wins when conversation loads resolve out of order", async () => {
  const { store, bridge, events } = setup();
  await store.mount();
  let resolve!: (value: typeof conversation) => void;
  bridge.handle(
    "chat.conversations.get",
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  const pending = store.select(conversation.id);
  store.newChat();
  resolve(conversation);
  await pending;
  expect(store.active).toBeNull();
  expect(store.loading).toBe(false);
  store.dispose();
  events.dispose();
});

test.each([
  [0, 500, 300, true],
  [500, 500, 1000, true],
  [452, 500, 1000, true],
  [451, 500, 1000, false],
  [100, 500, 1000, false],
])("scroll pinning at %s/%s/%s is %s", (top, height, total, expected) => {
  expect(shouldPinToBottom(Number(top), Number(height), Number(total))).toBe(expected);
});

test("a linked account whose family generates is offered in chat and becomes a provider on pick", async () => {
  const { store, bridge } = setup();
  const qwenId = "0199bb11-6666-7111-8111-000000000001" as AccountDto["id"];
  const signedOutId = "0199bb11-6666-7111-8111-000000000002" as AccountDto["id"];
  const account = (id: AccountDto["id"], adapter: AccountDto["adapter"]): AccountDto => ({
    id,
    adapter,
    emailMasked: null,
    displayName: null,
    avatarUrl: null,
    status: "linked",
    detail: null,
    expiresAt: null,
    lastCheckedAt: null,
    linkedProvidersCount: 0,
  });
  const created = provider({
    id: "0199bb11-6666-7111-8111-000000000003" as ProviderId,
    name: "Qwen",
    accountId: qwenId,
    models: [model({ id: MODEL, externalId: "qwen3.7-plus" })],
  });
  bridge
    .handle("accounts.list", () => [
      account(qwenId, "qwen-web"),
      { ...account(signedOutId, "qwen-web"), status: "needs-relink" },
    ])
    .handle("providers.adapters", () => ADAPTERS)
    .handle("providers.create", () => created)
    .handle("providers.probe", () => ({
      providerId: created.id,
      outcome: { kind: "ok", latencyMs: 1, models: [], live: true },
      status: "ok",
      statusDetail: null,
      checkedAt: 1,
      provider: created,
    }))
    .handle("providers.update", () => created)
    .handle("providers.get", () =>
      bridge.calls.some((call) => call.channel === "providers.create") ? created : saved,
    );
  await store.mount();
  expect(store.accountChoices.map((choice) => choice.label)).toEqual(["Qwen · аккаунт"]);
  expect(store.accountChoices[0]?.adapter).toBe("qwen-web");
  const connecting = store.connectAccount(qwenId);
  expect(store.connectingAccountId).toBe(qwenId);
  await connecting;
  expect(store.connectingAccountId).toBeNull();
  expect(store.error).toBeNull();
  const create = bridge.calls.find((call) => call.channel === "providers.create");
  expect(create?.payload).toMatchObject({
    adapter: "qwen-web",
    authMode: "account",
    accountId: qwenId,
    capabilities: ["text"],
    baseUrl: "https://chat.qwen.ai/api/v2",
  });
  const update = bridge.calls.find((call) => call.channel === "providers.update");
  expect(update?.payload).toMatchObject({
    id: created.id,
    settings: { selectedModelIds: ["qwen3.7-plus"] },
  });
  expect(store.provider?.id).toBe(created.id);
  expect(store.modelId).toBe("qwen3.7-plus");
  expect(store.accountChoices).toEqual([]);
  store.dispose();
});

test("an existing Qwen provider without models is reused, enabled and given its models", async () => {
  const { store, bridge } = setup();
  const qwenId = "0199bb11-6666-7111-8111-000000000001" as AccountDto["id"];
  const existingId = "0199bb11-6666-7111-8111-000000000004" as ProviderId;
  const bare = provider({
    id: existingId,
    name: "Qwen",
    adapter: "qwen-web",
    authMode: "account",
    accountId: qwenId,
    enabled: false,
    capabilities: ["embedding"],
    models: [],
  });
  const ready = {
    ...bare,
    enabled: true,
    models: [model({ id: MODEL, externalId: "qwen3.7-plus" })],
  };
  let probed = false;
  bridge
    .handle("accounts.list", () => [
      {
        id: qwenId,
        adapter: "qwen-web" as const,
        emailMasked: null,
        displayName: null,
        avatarUrl: null,
        status: "linked" as const,
        detail: null,
        expiresAt: null,
        lastCheckedAt: null,
        linkedProvidersCount: 1,
      },
    ])
    .handle("providers.adapters", () => ADAPTERS)
    .handle("providers.list", (input) =>
      input && "enabled" in input ? [saved] : [saved, { ...bare, modelCount: 0 }],
    )
    .handle("providers.probe", () => {
      probed = true;
      return {
        providerId: existingId,
        outcome: { kind: "ok", latencyMs: 1, models: [], live: true },
        status: "ok",
        statusDetail: null,
        checkedAt: 1,
        provider: ready,
      };
    })
    .handle("providers.update", () => ready)
    .handle("providers.get", (input) =>
      input.id === existingId ? (probed ? ready : bare) : saved,
    );
  await store.mount();
  expect(store.accountChoices.map((choice) => choice.accountId)).toEqual([qwenId]);
  await store.connectAccount(qwenId);
  expect(store.error).toBeNull();
  expect(bridge.calls.some((call) => call.channel === "providers.create")).toBe(false);
  expect(bridge.calls.find((call) => call.channel === "providers.update")?.payload).toMatchObject({
    id: existingId,
    enabled: true,
    capabilities: ["embedding", "text"],
    settings: { selectedModelIds: ["qwen3.7-plus"] },
  });
  expect(store.provider?.id).toBe(existingId);
  store.dispose();
});

test("a document selection rides along with the next message and then clears", async () => {
  const { store, bridge, events } = setup();
  await store.mount();
  const selection = DocumentSelectionDto.parse({
    documentId: "0199aa11-2222-7222-8222-222222222222",
    name: "Отчёт.pdf",
    page: 2,
    start: 4,
    end: 30,
    text: "Выручка составила 11,12 млрд руб.",
    untrusted: true,
  });
  store.attachSelection(selection);
  store.attachSelection(selection);
  expect(store.composer.selections).toEqual([selection]);
  store.composer.setText("Что это значит?");
  await store.send();
  const sent = bridge.calls.find((call) => call.channel === "chat.send");
  expect(sent?.payload).toMatchObject({ text: "Что это значит?", selections: [selection] });
  expect(store.composer.selections).toEqual([]);
  store.dispose();
  events.dispose();
});

test("attaching past the limit reports it instead of dropping silently", async () => {
  const { store, events } = setup();
  await store.mount();
  for (let start = 0; start < 9; start += 1)
    store.attachSelection(
      DocumentSelectionDto.parse({
        documentId: "0199aa11-2222-7222-8222-222222222222",
        name: "a.pdf",
        page: null,
        start,
        end: start + 1,
        text: "a",
        untrusted: true,
      }),
    );
  expect(store.composer.selections).toHaveLength(8);
  expect(store.error).toBe(SELECTIONS_FULL);
  store.dispose();
  events.dispose();
});

test("the last opened conversation is remembered and reopened on the next mount", async () => {
  const stored = new Map<string, Json>([["chat.conversation.last", id]]);
  const { store, bridge, events } = setup();
  bridge
    .handle("settings.get", ({ key }) => ({ key, value: stored.get(key) }))
    .handle("settings.set", ({ key, value }) => {
      stored.set(key, value);
      return { key, value, updatedAt: Date.now() };
    });
  await store.mount();
  expect(store.active?.id).toBe(id);
  expect(bridge.calls.some((call) => call.channel === "settings.set")).toBe(false);
  store.newChat();
  await vi.waitFor(() => expect(stored.get("chat.conversation.last")).toBeNull());
  store.dispose();
  const fresh = new ChatStore(createIpcClient(contract, bridge), events);
  await fresh.mount();
  expect(fresh.active).toBeNull();
  await fresh.select(conversation.id);
  await vi.waitFor(() => expect(stored.get("chat.conversation.last")).toBe(id));
  fresh.dispose();
  stored.set("chat.conversation.last", "0199bb11-4444-7111-8111-0000000000ff");
  const stale = new ChatStore(createIpcClient(contract, bridge), events);
  await stale.mount();
  expect(stale.active).toBeNull();
  stale.dispose();
  events.dispose();
});
