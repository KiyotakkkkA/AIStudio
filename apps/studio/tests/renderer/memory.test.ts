import { describe, expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  contract,
  type Contract,
  type ConversationId,
  type MemoryDto,
  type MemoryId,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import MemoryStore from "../../src/renderer/features/memory/MemoryStore.ts";
import {
  ALREADY_REMOVED,
  LOAD_ERROR,
  memoryCallSummary,
} from "../../src/renderer/features/memory/memoryCopy.ts";

const PLANS = "0199dd11-2222-7111-8111-000000000001" as ConversationId;
const NOTES = "0199dd11-2222-7111-8111-000000000002" as ConversationId;
const FULL = "Общая память заполнена: удалите ненужные записи";

function id(n: number): MemoryId {
  return `0199dd11-7777-7111-8111-${String(n).padStart(12, "0")}` as MemoryId;
}

function memory(n: number, overrides: Partial<MemoryDto> = {}): MemoryDto {
  return {
    id: id(n),
    scope: "account",
    conversation: null,
    content: `Факт ${String(n)}`,
    source: "user",
    createdAt: 1_000,
    updatedAt: 1_000,
    ...overrides,
  };
}

function dialog(n: number, chat: ConversationId, title: string): MemoryDto {
  return memory(n, { scope: "conversation", conversation: { id: chat, title } });
}

function setup(memories: MemoryDto[]) {
  const bridge = createFakeBridge<Contract>();
  let stored = [...memories];
  let next = 100;
  bridge
    .handle("memory.list", () => stored)
    .handle("chat.conversations.list", () => [])
    .handle("memory.create", (input) => {
      const created = memory(next++, {
        content: input.content,
        ...(input.conversationId === undefined
          ? {}
          : {
              scope: "conversation" as const,
              conversation: { id: input.conversationId, title: "Планы" },
            }),
      });
      stored = [created, ...stored];
      return created;
    })
    .handle("memory.update", ({ id: edited, content }) => {
      stored = stored.map((item) => (item.id === edited ? { ...item, content } : item));
      return stored.find((item) => item.id === edited) ?? memory(0);
    })
    .handle("memory.remove", ({ id: removed }) => {
      stored = stored.filter((item) => item.id !== removed);
    })
    .handle("memory.clear", (input) => {
      stored = stored.filter((item) =>
        input.conversationId === undefined
          ? item.scope !== "account"
          : item.conversation?.id !== input.conversationId,
      );
    });
  const store = new MemoryStore(createIpcClient(contract, bridge));
  return { bridge, store };
}

describe("MemoryStore", () => {
  test("splits account facts from facts grouped by dialog", async () => {
    const { store } = setup([
      memory(1),
      dialog(2, PLANS, "Планы"),
      dialog(3, NOTES, "Заметки"),
      dialog(4, PLANS, "Планы"),
    ]);
    await store.load();
    expect(store.account.map((item) => item.id)).toEqual([id(1)]);
    expect(
      store.dialogs.map((group) => [group.conversation.title, group.memories.map((m) => m.id)]),
    ).toEqual([
      ["Планы", [id(2), id(4)]],
      ["Заметки", [id(3)]],
    ]);
  });

  test("lists the facts of one dialog for the memory panel", async () => {
    const { store } = setup([memory(1), dialog(2, PLANS, "Планы"), dialog(3, NOTES, "Заметки")]);
    await store.load();
    expect(store.conversationFacts(PLANS).map((item) => item.id)).toEqual([id(2)]);
    expect(store.conversationFacts(NOTES).map((item) => item.id)).toEqual([id(3)]);
  });

  test("adds to the account or to a dialog, edits and removes, then reloads", async () => {
    const { bridge, store } = setup([memory(1)]);
    await store.load();
    expect(await store.add("Отвечать кратко")).toBe(true);
    expect(await store.add("Ветка main", PLANS)).toBe(true);
    expect(bridge.calls.filter((call) => call.channel === "memory.create")).toEqual([
      { channel: "memory.create", payload: { scope: "account", content: "Отвечать кратко" } },
      {
        channel: "memory.create",
        payload: { scope: "conversation", conversationId: PLANS, content: "Ветка main" },
      },
    ]);
    expect(store.account.map((item) => item.content)).toEqual(["Отвечать кратко", "Факт 1"]);
    expect(store.dialogs[0]?.memories.map((item) => item.content)).toEqual(["Ветка main"]);
    await store.edit(id(1), "Пишет на Rust");
    expect(store.account.find((item) => item.id === id(1))?.content).toBe("Пишет на Rust");
    await store.remove(id(1));
    expect(store.account.some((item) => item.id === id(1))).toBe(false);
  });

  test("clears one scope at a time", async () => {
    const { bridge, store } = setup([memory(1), dialog(2, PLANS, "Планы"), dialog(3, NOTES, "N")]);
    await store.load();
    await store.clear(PLANS);
    expect(store.dialogs.map((group) => group.conversation.id)).toEqual([NOTES]);
    await store.clear();
    expect(store.account).toEqual([]);
    expect(bridge.calls.filter((call) => call.channel === "memory.clear")).toEqual([
      { channel: "memory.clear", payload: { scope: "conversation", conversationId: PLANS } },
      { channel: "memory.clear", payload: { scope: "account" } },
    ]);
  });

  test("shows the host wording for a full scope and fixed copy for a missing fact", async () => {
    const { bridge, store } = setup([memory(1)]);
    await store.load();
    bridge.fail("memory.create", AppErrorCode.CONFLICT, FULL);
    expect(await store.add("Ещё один")).toBe(false);
    expect(store.error).toBe(FULL);
    bridge.fail("memory.remove", AppErrorCode.NOT_FOUND, "Запись памяти не найдена");
    await store.remove(id(1));
    expect(store.error).toBe(ALREADY_REMOVED);
  });

  test("a failed load reports the load error and keeps the list", async () => {
    const { bridge, store } = setup([memory(1)]);
    await store.load();
    bridge.fail("memory.list", AppErrorCode.UNKNOWN, "x");
    await store.load();
    expect(store.error).toBe(LOAD_ERROR);
    expect(store.account).toHaveLength(1);
  });
});

describe("memoryCallSummary", () => {
  test("describes each action and gives up on anything else", () => {
    expect(memoryCallSummary({ action: "save", scope: "account", content: "Пишет на Rust" })).toBe(
      "сохранено: «Пишет на Rust»",
    );
    expect(memoryCallSummary({ action: "update", ref: "a1", content: "Отвечать кратко" })).toBe(
      "изменено: «Отвечать кратко»",
    );
    expect(memoryCallSummary({ action: "forget", ref: "d2" })).toBe("удалена запись d2");
    expect(memoryCallSummary({ action: "shout" })).toBeNull();
    expect(memoryCallSummary("garbage")).toBeNull();
  });
});
