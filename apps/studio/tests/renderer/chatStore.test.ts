import { afterEach, expect, test, vi } from "vitest";
import { reaction } from "mobx";
import { createIpcClient } from "@zvs/ipc";
import {
  contract,
  ConversationDetailDto,
  RunHandleDto,
  MessageDto,
  type Contract,
  VectorStoreId,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import { shouldPinToBottom } from "../../src/renderer/features/chat/scroll.ts";
import { provider } from "./providerFixtures.ts";

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
