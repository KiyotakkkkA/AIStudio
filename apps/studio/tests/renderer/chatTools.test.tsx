import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  ChatToolCallDto,
  ConversationDetailDto,
  RunHandleDto,
  ToolDto,
  contract,
  type ChatContextToolDto,
  type Contract,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import ChatToolCalls from "../../src/renderer/features/chat/ChatToolCalls";
import {
  AUTOMATIC_TOOLS_TITLE,
  BASE_TOOLS_TITLE,
  COMMAND_TOOLS_TITLE,
  LOADED_TOOLS_TITLE,
  OFFERED_BY_COMMAND,
  OFFERED_BY_KNOWLEDGE,
  OFFERED_BY_MEMORY,
  OFFERED_ON_REQUEST,
  SOURCE_HINTS,
  toolSections,
} from "../../src/renderer/features/chat/toolSets.ts";
import {
  commandText,
  matchCommands,
  parseCommand,
} from "../../src/renderer/features/chat/chatCommands.ts";
import { durationLabel } from "../../src/renderer/features/chat/processCopy.ts";
import { provider } from "./providerFixtures.ts";

const saved = provider();
const handle = RunHandleDto.parse({
  id: "0199bb11-4444-7111-8111-000000000002",
  streamId: "0199bb11-4444-7111-8111-000000000003",
});
const conversation = ConversationDetailDto.parse({
  id: "0199bb11-4444-7111-8111-000000000001",
  title: "New conversation",
  providerId: saved.id,
  modelId: saved.models[0]!.externalId,
  messages: [],
  createdAt: 1,
  updatedAt: 1,
});
const tool = (key: string, extra: Partial<ToolDto> = {}) =>
  ToolDto.parse({
    id: `0199bb11-6666-7111-8111-00000000000${String(key.length % 10)}`,
    key,
    source: "mcp",
    sourceRef: null,
    displayName: key,
    description: `${key} description`,
    inputSchema: null,
    readOnly: true,
    destructive: false,
    tier: "auto",
    enabled: true,
    available: true,
    lastUsedAt: null,
    callCount: 0,
    registeredAt: 1,
    updatedAt: 1,
    ...extra,
  });
const approvalId = "0199bb11-5555-7111-8111-000000000002";
const pendingCall = ChatToolCallDto.parse({
  id: "call_1",
  key: "mcp.docs.search",
  name: "mcp__docs__search",
  round: 0,
  arguments: { query: "rotate" },
  status: "awaiting-approval",
  approvalId,
});
const browserUse = {
  command: "browser-use",
  description: "Работа во встроенном браузере: открытие и чтение страниц, действия на сайтах",
} as const;
const baseItems: ChatContextToolDto[] = [
  { key: "fs.read", tokens: 30, source: "base" },
  { key: "chat.ask", tokens: 20, source: "base" },
];

function setup() {
  const detail = structuredClone(conversation);
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [detail])
    .handle("chat.conversations.get", () => detail)
    .handle("chat.conversations.create", () => detail)
    .handle("chat.commands", () => [browserUse])
    .handle("providers.list", () => [saved])
    .handle("providers.get", () => saved)
    .handle("vectorStores.list", () => [])
    .handle("tools.list", () => [tool("mcp.docs.search"), tool("fs.read", { source: "builtin" })])
    .handle("chat.send", () => handle)
    .handle("chat.context", (input) => {
      const toolItems: ChatContextToolDto[] =
        input.command === "browser-use"
          ? [...baseItems, { key: "browser.navigate", tokens: 50, source: "command" }]
          : baseItems;
      const tools = toolItems.reduce((sum, item) => sum + item.tokens, 0);
      return {
        contextWindow: 8192,
        reservedOutput: 1024,
        system: 12,
        tools,
        toolItems,
        history: 0,
        historyMessages: 0,
        trimmed: 0,
        total: 12 + tools,
      };
    })
    .handle("runs.approve", () => undefined);
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new ChatStore(createIpcClient(contract, bridge), events);
  return { store, bridge, events };
}

test("the tools badge follows the host's breakdown and a command adds its tools", async () => {
  const { store, bridge, events } = setup();
  await store.mount();
  expect(store.commands).toEqual([browserUse]);
  await vi.waitFor(() => expect(store.contextUsed).toBe(62));
  expect(store.toolSections).toEqual([{ title: BASE_TOOLS_TITLE, items: baseItems }]);

  store.typeText("/br");
  store.completeCommand();
  await vi.waitFor(() => expect(store.contextUsed).toBe(112));
  expect(
    bridge.calls.filter((call) => call.channel === "chat.context").at(-1)?.payload,
  ).toMatchObject({ modelId: saved.models[0]!.externalId, command: "browser-use" });
  expect(store.toolSections.map((section) => section.title)).toEqual([
    BASE_TOOLS_TITLE,
    COMMAND_TOOLS_TITLE,
  ]);

  store.clearCommand();
  await vi.waitFor(() => expect(store.contextUsed).toBe(62));
  store.dispose();
  events.dispose();
});

test("a slash opens the command list, the keys move through it and a pick becomes a chip", async () => {
  const { store, events } = setup();
  await store.mount();
  expect(store.commandMatches).toEqual([]);

  store.typeText("/");
  expect(store.commandMatches).toEqual([browserUse]);
  store.moveCommand(1);
  expect(store.commandIndex).toBe(0);
  store.typeText("/brx");
  expect(store.commandMatches).toEqual([]);
  store.typeText("/browser-use now");
  expect(store.commandMatches).toEqual([]);

  store.typeText("/b");
  store.dismissCommands();
  expect(store.commandMatches).toEqual([]);
  store.typeText("/br");
  expect(store.commandMatches).toEqual([browserUse]);

  store.completeCommand();
  expect(store.composer.command).toBe("browser-use");
  expect(store.composer.text).toBe("");
  store.typeText("/");
  expect(store.commandMatches).toEqual([]);
  store.dispose();
  events.dispose();
});

test("a turn carries its command once and a typed command is understood on send", async () => {
  const { store, bridge, events } = setup();
  await store.mount();
  store.typeText("/b");
  store.completeCommand();
  store.typeText("Открой расписание");
  await store.send();
  expect(bridge.calls.find((call) => call.channel === "chat.send")?.payload).toMatchObject({
    text: "Открой расписание",
    command: "browser-use",
  });
  expect(store.composer.command).toBeNull();
  expect(
    bridge.calls.find((call) => call.channel === "chat.conversations.create")?.payload,
  ).not.toHaveProperty("enabledTools");
  events.dispatch({ type: "end", ...handle, seq: 1, ts: Date.now(), outcome: { status: "ok" } });
  await vi.waitFor(() => expect(store.generating).toBe(false));

  store.typeText("/browser-use");
  await vi.waitFor(() => expect(store.canSend).toBe(true));
  await store.send();
  expect(store.composer.command).toBe("browser-use");
  expect(bridge.calls.filter((call) => call.channel === "chat.send")).toHaveLength(1);

  store.clearCommand();
  store.typeText("/browser-use   Следующая страница ");
  await store.send();
  expect(bridge.calls.filter((call) => call.channel === "chat.send").at(-1)?.payload).toMatchObject(
    { text: "Следующая страница", command: "browser-use" },
  );
  events.dispatch({ type: "end", ...handle, seq: 2, ts: Date.now(), outcome: { status: "ok" } });
  await vi.waitFor(() => expect(store.generating).toBe(false));

  store.typeText("/unknown привет");
  await vi.waitFor(() => expect(store.canSend).toBe(true));
  await store.send();
  const last = bridge.calls.filter((call) => call.channel === "chat.send").at(-1)?.payload;
  expect(last).toMatchObject({ text: "/unknown привет" });
  expect(last?.command).toBeUndefined();
  store.dispose();
  events.dispose();
});

test("commands match by prefix and parse only when known", () => {
  expect(matchCommands([browserUse], "/BR")).toEqual([browserUse]);
  expect(matchCommands([browserUse], "br")).toEqual([]);
  expect(matchCommands([browserUse], "/br x")).toEqual([]);
  expect(parseCommand([browserUse], "  /Browser-Use  найди\nрасписание ")).toEqual({
    command: "browser-use",
    text: "найди\nрасписание",
  });
  expect(parseCommand([browserUse], "/browser-use")).toEqual({ command: "browser-use", text: "" });
  expect(parseCommand([browserUse], "/deploy now")).toBeNull();
  expect(parseCommand([browserUse], "no slash")).toBeNull();
  expect(commandText("browser-use")).toBe("/browser-use");
});

test("tool sections group the offered tools by why they were offered", () => {
  const items: ChatContextToolDto[] = [
    { key: "docs.search", tokens: 1, source: "loaded" },
    { key: "fs.read", tokens: 1, source: "base" },
    { key: "chat.memory", tokens: 1, source: "memory" },
    { key: "browser.navigate", tokens: 1, source: "command" },
    { key: "knowledge.search", tokens: 1, source: "knowledge" },
  ];
  expect(toolSections(items)).toEqual([
    { title: BASE_TOOLS_TITLE, items: [items[1]] },
    { title: AUTOMATIC_TOOLS_TITLE, items: [items[2], items[4]] },
    { title: COMMAND_TOOLS_TITLE, items: [items[3]] },
    { title: LOADED_TOOLS_TITLE, items: [items[0]] },
  ]);
  expect(toolSections([])).toEqual([]);
  expect(SOURCE_HINTS).toEqual({
    base: null,
    knowledge: OFFERED_BY_KNOWLEDGE,
    memory: OFFERED_BY_MEMORY,
    command: OFFERED_BY_COMMAND,
    loaded: OFFERED_ON_REQUEST,
  });
});

test("live text and reasoning are split into rounds at each tool call", async () => {
  const { store, events } = setup();
  await store.mount();
  store.composer.setText("Question");
  await store.send();
  const at = { ...handle, ts: Date.now() };
  events.dispatch({ type: "token", ...at, seq: 1, delta: "Мысль.", kind: "reasoning" });
  events.dispatch({ type: "token", ...at, seq: 2, delta: "Поиск." });
  events.dispatch({
    type: "step",
    ...at,
    seq: 3,
    step: { domain: "tool-call", index: 0, call: { ...pendingCall, status: "running" } },
  });
  events.dispatch({ type: "token", ...at, seq: 4, delta: "Ещё.", kind: "reasoning" });
  events.dispatch({ type: "token", ...at, seq: 5, delta: "\n\nИтог." });
  events.dispatch({
    type: "step",
    ...at,
    seq: 6,
    step: {
      domain: "tool-call",
      index: 1,
      call: { ...pendingCall, id: "call_2", round: 1, status: "running" },
    },
  });

  expect(store.liveRounds).toEqual([
    { text: "Поиск.", reasoning: "Мысль." },
    { text: "\n\nИтог.", reasoning: "Ещё." },
  ]);
  expect(store.liveText).toBe("Поиск.\n\nИтог.");
  store.dispose();
  events.dispose();
});

test("live tool-call events drive the cards and keep their approval out of the generic bar", async () => {
  const { store, events, bridge } = setup();
  await store.mount();
  store.composer.setText("Question");
  await store.send();

  events.dispatch({
    type: "step",
    ...handle,
    seq: 1,
    ts: Date.now(),
    step: { domain: "tool-call", index: 0, call: { ...pendingCall, status: "running" } },
  });
  events.dispatch({
    type: "step",
    ...handle,
    seq: 2,
    ts: Date.now(),
    step: { domain: "tool-call", index: 0, call: pendingCall },
  });
  events.dispatch({
    type: "approval",
    ...handle,
    seq: 3,
    ts: Date.now(),
    request: { id: approvalId, runId: handle.id, subject: "mcp.docs.search" },
  });
  events.dispatch({
    type: "approval",
    ...handle,
    seq: 4,
    ts: Date.now(),
    request: { id: "0199bb11-5555-7111-8111-000000000009", runId: handle.id, subject: "other" },
  });

  expect(store.liveToolCalls).toEqual([pendingCall]);
  expect(store.approvalFor(approvalId)).toMatchObject({ id: approvalId, busy: false });
  expect(store.standaloneApprovals.map((request) => request.subject)).toEqual(["other"]);

  await store.decideApproval(approvalId, true);
  expect(bridge.calls.at(-1)).toMatchObject({
    channel: "runs.approve",
    payload: { id: handle.id, approvalId, always: false },
  });
  store.dispose();
  events.dispose();
});

test("a card shows the call, its result and an inline approval prompt", () => {
  const onDecide = vi.fn();
  const { rerender } = render(
    <ChatToolCalls
      calls={[pendingCall]}
      approvalFor={(id) => (id === approvalId ? { id, busy: false } : undefined)}
      onDecide={onDecide}
    />,
  );
  expect(screen.getByText("ждёт подтверждения")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Разрешить один раз" }));
  expect(onDecide).toHaveBeenCalledWith(approvalId, true);
  fireEvent.click(screen.getByRole("button", { name: "Запретить" }));
  expect(onDecide).toHaveBeenCalledWith(approvalId, false);

  rerender(
    <ChatToolCalls
      calls={[
        {
          ...pendingCall,
          status: "succeeded",
          approvalId: null,
          result: "Use the CLI.",
          truncated: true,
          durationMs: 1500,
        },
      ]}
    />,
  );
  expect(screen.queryByRole("button", { name: "Разрешить один раз" })).toBeNull();
  expect(screen.getByText("готово")).toBeTruthy();
  expect(screen.getByText("Use the CLI.")).toBeTruthy();
  expect(screen.getByText("Результат · сокращён")).toBeTruthy();
  expect(screen.getByText(durationLabel(1500))).toBeTruthy();
});
