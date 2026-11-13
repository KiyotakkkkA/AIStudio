import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  ChatAttachDto,
  ChatCompactionDto,
  ConversationDetailDto,
  ConversationId,
  RunHandleDto,
  contract,
  type AssistantPreferencesDto,
  type Contract,
  type ProviderDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import { menuEntries, parseAction } from "../../src/renderer/features/chat/chatCommands.ts";
import {
  COMPACT_ACTION,
  COMPACT_LABEL,
  compactionMessages,
} from "../../src/renderer/features/chat/compactionCopy.ts";
import AssistantPreferencesStore from "../../src/renderer/features/assistant/AssistantPreferencesStore.ts";
import { ASSISTANT_SAVE_FAILED } from "../../src/renderer/features/assistant/assistantCopy.ts";
import { MODEL, OLLAMA, OTHER_MODEL, model, provider, summary } from "./providerFixtures.ts";

const saved = provider();
const ID = ConversationId.parse("0199ff11-4444-7111-8111-000000000001");
const ANSWER = "0199ff11-7777-7111-8111-000000000002";
const handle = RunHandleDto.parse({
  id: "0199ff11-4444-7111-8111-000000000002",
  streamId: "0199ff11-4444-7111-8111-000000000003",
});

function message(id: string, role: "user" | "assistant", content: string, createdAt: number) {
  return {
    id,
    conversationId: ID,
    role,
    content,
    citations: [],
    tokensIn: 0,
    tokensOut: 0,
    usageEstimated: true,
    durationMs: 0,
    runId: null,
    partial: false,
    createdAt,
  };
}

const plain = ConversationDetailDto.parse({
  id: ID,
  title: "Сборка",
  providerId: saved.id,
  modelId: saved.models[0]!.externalId,
  createdAt: 1,
  updatedAt: 1,
  messages: [
    message("0199ff11-7777-7111-8111-000000000001", "user", "Настрой сборку", 1),
    message(ANSWER, "assistant", "Готово", 2),
  ],
});

const compaction = ChatCompactionDto.parse({
  id: "0199ff11-9999-7111-8111-000000000001",
  boundaryMessageId: ANSWER,
  summary: "Сборка настроена.",
  messages: 2,
  tokensBefore: 900,
  tokensAfter: 40,
  providerId: saved.id,
  modelId: "m",
  trigger: "manual",
  createdAt: 3,
});

const compacted = ConversationDetailDto.parse({ ...plain, compactions: [compaction] });

function setup(attached: ChatAttachDto | null = null) {
  let current = plain;
  let live = attached;
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [plain])
    .handle("chat.conversations.get", () => current)
    .handle("chat.attach", () => live)
    .handle("providers.list", () => [saved])
    .handle("providers.get", () => saved)
    .handle("vectorStores.list", () => [])
    .handle("chat.cancel", () => undefined)
    .handle("chat.compact", () => {
      live = null;
      return handle;
    })
    .handle("chat.compactions.undo", () => {
      current = plain;
      return plain;
    });
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new ChatStore(createIpcClient(contract, bridge), events);
  let seq = 1;
  return {
    bridge,
    events,
    store,
    serve(value: ConversationDetailDto) {
      current = value;
    },
    end(status: "ok" | "cancelled" | "failed") {
      events.dispatch({ type: "end", ...handle, seq: 9, ts: 1, outcome: { status } });
    },
    step(step: Record<string, unknown>, streamId: string = handle.streamId) {
      seq += 1;
      events.dispatch({ type: "step", id: handle.id, streamId, seq, ts: 1, step });
    },
  };
}

async function opened(attached: ChatAttachDto | null = null) {
  const context = setup(attached);
  await context.store.mount();
  await context.store.select(ID);
  return context;
}

test("the slash menu offers /compact only for a dialog that can be compacted", () => {
  const input = {
    commands: [],
    accounts: [],
    attachedCommands: [],
    attachedPlugins: [],
    text: "/comp",
    picking: null,
  };
  expect(menuEntries(input)).toEqual([]);
  const entries = menuEntries({ ...input, compactable: true });
  expect(entries).toEqual([
    expect.objectContaining({ type: "action", action: COMPACT_ACTION, label: COMPACT_LABEL }),
  ]);
  expect(menuEntries({ ...input, text: "/goal", compactable: true })).toEqual([]);
  expect(parseAction("/compact")).toEqual({ action: COMPACT_ACTION, focus: "" });
  expect(parseAction("  /COMPACT   решения по API ")).toEqual({
    action: COMPACT_ACTION,
    focus: "решения по API",
  });
  expect(parseAction("/compactor")).toBeNull();
  expect(parseAction("сжать")).toBeNull();
  expect(compactionMessages(21)).toBe("21 сообщение");
  expect(compactionMessages(1_204)).toBe(`${(1_204).toLocaleString("ru-RU")} сообщения`);
});

test("/compact with a focus starts the job, and its end reloads the dialog with the marker", async () => {
  const { bridge, store, serve, end, step } = await opened();
  expect(store.canCompact).toBe(true);
  store.typeText("/compact решения по API");
  await store.send();
  expect(bridge.calls).toContainEqual({
    channel: "chat.compact",
    payload: { conversationId: ID, instructions: "решения по API" },
  });
  expect(store.compacting).toBe(true);
  expect(store.generating).toBe(true);
  expect(store.run).toEqual(handle);
  expect(store.composer.text).toBe("");
  expect(bridge.calls.some((call) => call.channel === "chat.send")).toBe(false);

  step({ domain: "compaction", conversationId: ID, trigger: "manual", phase: "started" });
  serve(compacted);
  end("ok");
  await vi.waitFor(() => expect(store.active?.compactions).toEqual([compaction]));
  expect(store.compacting).toBe(false);
  expect(store.generating).toBe(false);
  expect(store.compactionEnded).toBe(true);

  await store.undoCompaction(compaction.id);
  expect(bridge.calls).toContainEqual({
    channel: "chat.compactions.undo",
    payload: { conversationId: ID, compactionId: compaction.id },
  });
  expect(store.active?.compactions).toEqual([]);
  store.dispose();
});

test("choosing /compact in the menu runs it at once and a stopped job leaves the dialog alone", async () => {
  const { bridge, store, end } = await opened();
  store.typeText("/comp");
  expect(store.commandMatches.map((entry) => entry.type)).toEqual(["action"]);
  store.completeCommand();
  await vi.waitFor(() => expect(store.run).toEqual(handle));
  expect(bridge.calls).toContainEqual({ channel: "chat.compact", payload: { conversationId: ID } });
  await store.stop();
  expect(bridge.calls).toContainEqual({ channel: "chat.cancel", payload: { id: handle.id } });
  end("cancelled");
  expect(store.outcome).toBe("cancelled");
  expect(store.compactionEnded).toBe(true);
  expect(store.compacting).toBe(false);
  store.dispose();
});

test("a reopened dialog that is being compacted shows the compaction instead of a live answer", async () => {
  const { store } = await opened(
    ChatAttachDto.parse({
      run: handle,
      seq: null,
      startedAt: 5,
      content: "",
      reasoning: "",
      toolCalls: [],
      approvals: [],
      questions: [],
      compacting: true,
    }),
  );
  expect(store.compacting).toBe(true);
  expect(store.generating).toBe(true);
  expect(store.canCompact).toBe(false);
  store.dispose();
});

test("an automatic compaction inside a turn is reported and a recap lands on its message", async () => {
  const { store, step } = await opened(
    ChatAttachDto.parse({
      run: handle,
      seq: 1,
      startedAt: 5,
      content: "",
      reasoning: "",
      toolCalls: [],
      approvals: [],
      questions: [],
    }),
  );
  step({ domain: "compaction", conversationId: ID, trigger: "auto", phase: "started" });
  expect(store.autoCompacting).toBe(true);
  expect(store.compacting).toBe(false);
  step({ domain: "compaction", conversationId: ID, trigger: "auto", phase: "finished" });
  expect(store.autoCompacting).toBe(false);

  step(
    { domain: "recap", conversationId: ID, messageId: ANSWER, recap: "Сборка настроена." },
    "0199ff11-4444-7111-8111-000000000099",
  );
  expect(store.active?.messages.find((item) => item.id === ANSWER)?.recap).toBe(
    "Сборка настроена.",
  );
  store.dispose();
});

function preferencesSetup(providers: ProviderDto[] = [provider()]) {
  let stored: AssistantPreferencesDto = {
    autoCompact: true,
    recaps: true,
    summarizerModels: [],
  };
  const saved: AssistantPreferencesDto[] = [];
  let refusal: Error | null = null;
  const bridge = createFakeBridge<Contract>()
    .handle("providers.list", () => providers.map((item) => summary({ id: item.id })))
    .handle("providers.get", ({ id }) => providers.find((item) => item.id === id) ?? provider())
    .handle("assistant.get", () => stored)
    .handle("assistant.set", (value) => {
      if (refusal !== null) throw refusal;
      saved.push(value);
      stored = value;
      return value;
    });
  const store = new AssistantPreferencesStore(createIpcClient(contract, bridge));
  return {
    store,
    saved,
    refuse(error: Error) {
      refusal = error;
    },
  };
}

test("assistant preferences toggle both features and keep an ordered summarizer list", async () => {
  const { store, saved } = preferencesSetup([
    provider({
      models: [
        model({ id: MODEL, externalId: "qwen3:8b" }),
        model({ id: OTHER_MODEL, externalId: "gpt-oss:20b" }),
      ],
    }),
  ]);
  await store.load();
  expect(store.loaded).toBe(true);
  await store.setAutoCompact(false);
  await store.setRecaps(false);
  expect(saved.at(-1)).toEqual({ autoCompact: false, recaps: false, summarizerModels: [] });

  store.selectDraftProvider(OLLAMA);
  await store.addDraft();
  store.selectDraftProvider(OLLAMA);
  store.setDraftModel("gpt-oss:20b");
  await store.addDraft();
  await store.move(1, -1);
  expect(saved.at(-1)?.summarizerModels).toEqual([
    { providerId: OLLAMA, modelId: "gpt-oss:20b" },
    { providerId: OLLAMA, modelId: "qwen3:8b" },
  ]);
  expect(store.views.map((view) => view.modelName)).toHaveLength(2);
  await store.remove(0);
  expect(store.models).toEqual([{ providerId: OLLAMA, modelId: "qwen3:8b" }]);
});

test("a refused save restores the stored preferences and shows a generic message", async () => {
  const { store, refuse } = preferencesSetup();
  await store.load();
  refuse(new Error("offline"));
  await store.setRecaps(false);
  expect(store.recaps).toBe(true);
  expect(store.error).toBe(ASSISTANT_SAVE_FAILED);
});
