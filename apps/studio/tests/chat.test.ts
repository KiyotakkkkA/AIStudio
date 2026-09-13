import { afterEach, beforeEach, expect, test, vi } from "vitest";
import {
  CreateConversationInput,
  HostEvent,
  VectorStoreId,
  contract,
  type VectorSearchHitDto,
} from "@zvs/shared";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { createFakeDriver, type FakeDriver } from "../../../test/helpers/FakeDriver.ts";
import { NodeRegistry } from "../src/host/kernel/NodeRegistry.ts";
import { registerCoreNodes } from "../src/host/kernel/coreNodes.ts";
import { createEventBus, type EventBus } from "../src/host/platform/events.ts";
import { createId } from "../src/host/platform/ids.ts";
import { RunService } from "../src/host/services/RunService.ts";
import { ChatService } from "../src/host/services/ChatService.ts";
import { estimateTokens, windowChatHistory } from "../src/host/services/chatHistory.ts";
import { createChatHandlers } from "../src/host/ipc/chat.ts";
import { openDatabase } from "../src/host/data/client.ts";

let db: TemporaryDatabase;
let driver: FakeDriver;
let runs: RunService;
let chat: ChatService;
let bus: EventBus;
let events: HostEvent[];
let providerId: string;
let modelId: string;
const search = vi.fn<(id: string, query: string) => Promise<VectorSearchHitDto[]>>();
const log = vi.fn();

beforeEach(() => {
  db = temporaryDatabase();
  events = [];
  search.mockReset().mockResolvedValue([]);
  log.mockClear();
  driver = createFakeDriver();
  const registry = registerCoreNodes(new NodeRegistry());
  bus = createEventBus({
    senders: () => [
      { isDestroyed: () => false, send: (_channel, event) => events.push(HostEvent.parse(event)) },
    ],
  });
  runs = new RunService({
    data: db.client,
    registry,
    events: bus,
    services: {
      providers: { text: async () => driver.text, ephemeralDriver: async () => driver },
      vectorStores: { search },
    },
  });
  chat = new ChatService({ data: db.client, registry, runs, logger: { log, close() {} } });
  runs.permissions.grant("llm.generate", "global", "auto", "test");
  runs.permissions.grant("vector.search", "global", "auto", "test");
  providerId = db.client.repositories.providers.create({
    name: "Fake",
    kind: "ollama",
    baseUrl: "http://localhost:11434",
    capabilities: ["text", "embedding"],
    createdAt: 1,
    updatedAt: 1,
  }).id;
  modelId = db.client.repositories.models.replaceForProvider(providerId, [
    {
      externalId: "fake-model",
      displayName: "Fake",
      contextWindow: 1024,
      maxOutput: 128,
      discoveredAt: 1,
    },
  ])[0]!.id;
});
afterEach(async () => {
  await runs.dispose();
  bus.dispose();
  db.dispose();
});
function conversation(extra: Record<string, unknown> = {}) {
  return chat.create(CreateConversationInput.parse({ providerId, modelId, ...extra }));
}
function store() {
  const id = createId();
  db.client.repositories.vectorStores.create({
    id,
    name: id,
    embeddingProviderId: providerId,
    embeddingModelId: "embed",
    dimension: 1,
    metric: "cosine",
    chunkSize: 100,
    chunkOverlap: 0,
    createdAt: 1,
    updatedAt: 1,
  });
  return VectorStoreId.parse(id);
}
const hit: VectorSearchHitDto = {
  id: "chunk",
  documentId: "document",
  path: "docs/source.md",
  chunkIndex: 3,
  score: 0.9,
  payload: { text: "A retrieved fact." },
};

test("a storeless channel turn returns before tokens, skips retrieval, persists usage and monotonic events", async () => {
  const handlers = createChatHandlers(chat);
  const created = conversation({ systemPrompt: "Be concise." });
  const handle = await handlers["chat.send"]({
    conversationId: created.id,
    text: "First question",
  });
  expect(events.filter((event) => event.type === "token")).toHaveLength(0);
  expect(contract["chat.send"].output.parse(handle)).toEqual(handle);
  await runs.wait(handle.id);
  expect(runs.get(handle.id)).toMatchObject({
    kind: "chat",
    subjectId: created.id,
    status: "succeeded",
  });
  expect(search).not.toHaveBeenCalled();
  expect(runs.steps(handle.id).map((step) => step.type)).toEqual([
    "chat.prepare",
    "llm.generate",
    "chat.persist",
  ]);
  const detail = await handlers["chat.conversations.get"]({ id: created.id });
  expect(detail.title).toBe("First question");
  expect(detail.messages).toHaveLength(2);
  expect(detail.messages[1]).toMatchObject({
    role: "assistant",
    content: "Hello world",
    partial: false,
    runId: handle.id,
    citations: [],
    usageEstimated: true,
  });
  expect(detail.messages[1]!.tokensIn).toBeGreaterThan(0);
  expect(detail.messages[1]!.tokensOut).toBeGreaterThan(0);
  expect(driver.requests[0]).toMatchObject({
    model: "fake-model",
    system: "Be concise.",
    messages: [{ role: "user", content: "First question" }],
  });
  const sequence = events
    .filter((event) => event.streamId === handle.streamId)
    .map((event) => event.seq);
  expect(sequence).toEqual(sequence.map((_, index) => index));
  expect(events.filter((event) => event.type === "token").map((event) => event.delta)).toEqual([
    "Hello",
    " world",
  ]);
});

test("attached stores search in parallel and only supplied passages become persisted citations", async () => {
  const stores = [store(), store()];
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  search.mockImplementation(async () => {
    await gate;
    return [hit];
  });
  const created = conversation({ attachedStoreIds: stores });
  const handle = chat.sendMessage(created.id, "Find a fact");
  await vi.waitFor(() => expect(search).toHaveBeenCalledTimes(2));
  expect(driver.requests).toHaveLength(0);
  release();
  await runs.wait(handle.id);
  expect(runs.get(handle.id).status).toBe("succeeded");
  expect(chat.get(created.id).messages[1]!.citations).toEqual(
    stores.map((storeId) => ({
      storeId,
      documentId: hit.documentId,
      sourcePath: hit.path,
      chunkIndex: 3,
      score: 0.9,
    })),
  );
  expect(
    driver.requests[0]!.messages.some((message) => message.content.includes("A retrieved fact.")),
  ).toBe(true);
});

test("cancelling mid-stream persists the prefix before the terminal event and releases the turn", async () => {
  driver.script({ manual: true });
  const created = conversation();
  const handle = chat.sendMessage(created.id, "Question");
  await driver.whenHolding();
  driver.release();
  await vi.waitFor(() => expect(driver.emitted).toBe(1));
  await driver.whenHolding();
  expect(() => chat.sendMessage(created.id, "Concurrent")).toThrow("active turn");
  expect(() => chat.remove(created.id)).toThrow("active turn");
  await chat.cancel(handle.id);
  const partial = chat.get(created.id).messages[1]!;
  expect(partial).toMatchObject({ content: "Hello", partial: true, runId: handle.id });
  expect(runs.get(handle.id).status).toBe("cancelled");
  expect(events.at(-1)).toMatchObject({ type: "end", outcome: { status: "cancelled" } });
  driver.script({ manual: false });
  const next = chat.sendMessage(created.id, "Continue");
  await runs.wait(next.id);
  expect(runs.get(next.id).status).toBe("succeeded");
});

test("provider failure preserves a partial answer and fails the kernel run", async () => {
  driver.script({ failAfter: 1, failWith: new Error("connection lost") });
  const created = conversation();
  const handle = chat.sendMessage(created.id, "Question");
  await runs.wait(handle.id);
  expect(runs.get(handle.id).status).toBe("failed");
  expect(chat.get(created.id).messages[1]).toMatchObject({ content: "Hello", partial: true });
});

test("shutdown preserves streamed content on disk and recovery does not repeat generation", async () => {
  driver.script({ manual: true });
  const created = conversation();
  const handle = chat.sendMessage(created.id, "Question");
  await driver.whenHolding();
  driver.release();
  await vi.waitFor(() => expect(driver.emitted).toBe(1));
  await runs.dispose();
  const reopened = openDatabase({ file: db.file });
  try {
    expect(reopened.repositories.chat.messages(created.id)[1]).toMatchObject({
      content: "Hello",
      partial: true,
    });
    const registry = registerCoreNodes(new NodeRegistry());
    bus.dispose();
    bus = createEventBus();
    runs = new RunService({ data: db.client, registry, events: bus });
    chat = new ChatService({ data: db.client, registry, runs });
    runs.recover();
    expect(runs.get(handle.id).status).toBe("interrupted");
    expect(driver.requests).toHaveLength(1);
  } finally {
    reopened.close();
  }
});

test("immediate cancellation never calls the model or creates a phantom assistant answer", async () => {
  const created = conversation();
  const handle = chat.sendMessage(created.id, "Question");
  await chat.cancel(handle.id);
  expect(runs.get(handle.id).status).toBe("cancelled");
  expect(driver.requests).toHaveLength(0);
  expect(chat.get(created.id).messages).toHaveLength(1);
});

test("windowing keeps system and current turn, logs trimming, and sends a bounded recent history", async () => {
  const created = conversation({ systemPrompt: "Always be helpful." });
  for (let index = 0; index < 8; index++) {
    const handle = chat.sendMessage(created.id, `${index} ${"past ".repeat(40)}`);
    await runs.wait(handle.id);
  }
  const handle = chat.sendMessage(created.id, "current turn");
  await runs.wait(handle.id);
  const request = driver.requests.at(-1)!;
  expect(request.system).toBe("Always be helpful.");
  expect(request.messages.at(-1)).toEqual({ role: "user", content: "current turn" });
  const used =
    estimateTokens(request.system!) +
    8 +
    request.messages.reduce((sum, item) => sum + estimateTokens(item.content) + 8, 0);
  expect(used + request.maxOutputTokens!).toBeLessThanOrEqual(1024);
  expect(request.messages.length).toBeLessThan(17);
  expect(log).toHaveBeenCalledWith(
    "info",
    "chat",
    "Chat history trimmed",
    expect.objectContaining({ trimmed: expect.any(Number) }),
  );
  expect(
    events.some(
      (event) =>
        event.type === "log" && JSON.stringify(event.line).includes("Chat history trimmed"),
    ),
  ).toBe(true);
});

test("oversized mandatory prompts fail before persisting or launching a run", () => {
  const created = conversation({ systemPrompt: "s".repeat(900) });
  expect(() => chat.sendMessage(created.id, "current")).toThrow("context budget");
  expect(chat.get(created.id).messages).toEqual([]);
  expect(runs.list()).toEqual([]);
  expect(() =>
    windowChatHistory("system", [], { role: "user", content: "я".repeat(50) }, 100, 20),
  ).toThrow("context budget");
});

test("retrieval excludes passages that cannot fit and does not cite omitted content", () => {
  const prepared = windowChatHistory("system", [], { role: "user", content: "current" }, 100, 20, [
    {
      storeId: VectorStoreId.parse(createId()),
      hit: { ...hit, payload: { text: "x".repeat(1000) } },
    },
  ]);
  expect(prepared.citations).toEqual([]);
  expect(prepared.messages).toEqual([{ role: "user", content: "current" }]);
});

test("conversation channels rename, list and cascade message deletion while retaining run history", async () => {
  const handlers = createChatHandlers(chat);
  const created = conversation();
  const handle = chat.sendMessage(created.id, "Question");
  await runs.wait(handle.id);
  await handlers["chat.conversations.rename"]({ id: created.id, title: "Custom title" });
  expect(await handlers["chat.conversations.list"]()).toEqual([
    expect.objectContaining({ title: "Custom title" }),
  ]);
  await handlers["chat.conversations.remove"]({ id: created.id });
  expect(chat.list()).toEqual([]);
  expect(db.client.repositories.chat.messages(created.id)).toEqual([]);
  expect(runs.get(handle.id).status).toBe("succeeded");
});

test("permission denial prevents generation and leaves the answer partial", async () => {
  runs.permissions.grant("llm.generate", "global", "off", "test");
  const created = conversation();
  const handle = chat.sendMessage(created.id, "Question");
  await runs.wait(handle.id);
  expect(runs.get(handle.id).status).toBe("failed");
  expect(driver.requests).toHaveLength(0);
  expect(chat.get(created.id).messages[1]).toMatchObject({ partial: true, content: "" });
});
