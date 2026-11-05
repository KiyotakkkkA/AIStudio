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
  type IntegrationAccountDto,
  type IntegrationId,
  type IntegrationKind,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore, { BROWSER_NAVIGATE_TOOL } from "../../src/renderer/features/chat/ChatStore.ts";
import ChatToolCalls from "../../src/renderer/features/chat/ChatToolCalls";
import {
  AUTOMATIC_TOOLS_TITLE,
  BASE_TOOLS_TITLE,
  COMMAND_TOOLS_TITLE,
  LOADED_TOOLS_TITLE,
  OFFERED_BY_COMMAND,
  OFFERED_BY_KNOWLEDGE,
  OFFERED_BY_MEMORY,
  OFFERED_BY_SUBAGENTS,
  OFFERED_ON_REQUEST,
  SOURCE_HINTS,
  toolSections,
} from "../../src/renderer/features/chat/toolSets.ts";
import {
  commandMark,
  commandText,
  menuEntries,
  parseCommands,
  slashQuery,
  withoutSlashToken,
} from "../../src/renderer/features/chat/chatCommands.ts";
import { durationLabel } from "../../src/renderer/features/chat/processCopy.ts";
import { pluginToolKind } from "../../src/renderer/features/chat/toolCallCopy.ts";
import { PLUGIN_MARKS } from "../../src/renderer/features/plugins/pluginPresentation.ts";
import ChatCommandMenu from "../../src/renderer/features/chat/ChatCommandMenu";
import { mdiFlagOutline, mdiWeb } from "@mdi/js";
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
const goal = {
  command: "goal",
  description: "Цель",
} as const;
function account(
  id: string,
  kind: IntegrationKind,
  name: string,
  enabled = true,
): IntegrationAccountDto {
  return {
    id: `0199aa22-2222-7222-8222-00000000000${id}` as IntegrationId,
    kind,
    name,
    enabled,
    config: {},
    status: enabled ? "running" : "disabled",
    statusDetail: null,
    lastActivityAt: null,
    counters: { events: 0, duplicates: 0, runs: 0, toolCalls: 0, errors: 0 },
    createdAt: 1,
    updatedAt: 1,
  };
}
const work = account("1", "github", "Работа");
const personal = account("2", "github", "Личный");
const bot = account("3", "telegram", "Бот");
const off = account("4", "gitlab", "Отключён", false);
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
    .handle("chat.commands", () => [browserUse, goal])
    .handle("integrations.list", () => [work, personal, bot, off])
    .handle("providers.list", () => [saved])
    .handle("providers.get", () => saved)
    .handle("vectorStores.list", () => [])
    .handle("tools.list", () => [tool("mcp.docs.search"), tool("fs.read", { source: "builtin" })])
    .handle("chat.send", () => handle)
    .handle("chat.context", (input) => {
      const toolItems: ChatContextToolDto[] = input.commands.includes("browser-use")
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
  expect(store.commands).toEqual([browserUse, goal]);
  await vi.waitFor(() => expect(store.contextUsed).toBe(62));
  expect(store.toolSections).toEqual([{ title: BASE_TOOLS_TITLE, items: baseItems }]);

  store.typeText("/br");
  store.completeCommand();
  await vi.waitFor(() => expect(store.contextUsed).toBe(112));
  expect(
    bridge.calls.filter((call) => call.channel === "chat.context").at(-1)?.payload,
  ).toMatchObject({
    modelId: saved.models[0]!.externalId,
    commands: ["browser-use"],
    plugins: [],
  });
  expect(store.toolSections.map((section) => section.title)).toEqual([
    BASE_TOOLS_TITLE,
    COMMAND_TOOLS_TITLE,
  ]);

  store.removeCommand("browser-use");
  await vi.waitFor(() => expect(store.contextUsed).toBe(62));
  store.dispose();
  events.dispose();
});

test("a slash lists commands and plugins, a pick becomes a chip and more can follow", async () => {
  const { store, events } = setup();
  await store.mount();
  expect(store.commandMatches).toEqual([]);

  store.typeText("/");
  expect(store.commandMatches.map((entry) => entry.label)).toEqual([
    "/browser-use",
    "/goal",
    "/github",
    "/telegram",
  ]);
  store.moveCommand(-1);
  expect(store.commandIndex).toBe(3);
  store.typeText("/brx");
  expect(store.commandMatches).toEqual([]);

  store.typeText("/b");
  store.dismissCommands();
  expect(store.commandMatches).toEqual([]);
  store.typeText("Найди /br");
  store.completeCommand();
  expect(store.composer.commands).toEqual(["browser-use"]);
  expect(store.composer.text).toBe("Найди ");

  store.typeText("Найди /");
  expect(store.commandMatches.map((entry) => entry.label)).toEqual([
    "/goal",
    "/github",
    "/telegram",
  ]);
  store.typeText("Найди /go");
  store.completeCommand();
  expect(store.composer.commands).toEqual(["browser-use", "goal"]);

  store.typeText("Найди /tel");
  store.completeCommand();
  expect(store.composer.plugins).toEqual([bot.id]);

  store.typeText("Найди /gith");
  store.completeCommand();
  expect(store.pickingPlugin).toBe("github");
  expect(store.composer.text).toBe("Найди ");
  expect(store.commandMatches.map((entry) => entry.label)).toEqual(["Работа", "Личный"]);
  store.moveCommand(1);
  store.completeCommand();
  expect(store.pickingPlugin).toBeNull();
  expect(store.composer.plugins).toEqual([bot.id, personal.id]);
  expect(store.attachedPlugins.map((entry) => entry.name)).toEqual(["Бот", "Личный"]);

  store.typeText("Найди /gith");
  store.completeCommand();
  expect(store.composer.plugins).toEqual([bot.id, personal.id, work.id]);
  store.typeText("/git");
  expect(store.commandMatches).toEqual([]);

  store.typeText("");
  store.removeLastAttachment();
  expect(store.composer.plugins).toEqual([bot.id, personal.id]);
  store.dispose();
  events.dispose();
});

test("a turn carries its commands and plugins once and typed commands are understood on send", async () => {
  const { store, bridge, events } = setup();
  await store.mount();
  store.typeText("/b");
  store.completeCommand();
  store.typeText("/tel");
  store.completeCommand();
  store.typeText("Открой расписание");
  await store.send();
  expect(bridge.calls.find((call) => call.channel === "chat.send")?.payload).toMatchObject({
    text: "Открой расписание",
    commands: ["browser-use"],
    plugins: [bot.id],
  });
  expect(store.composer.commands).toEqual([]);
  expect(store.composer.plugins).toEqual([]);
  expect(
    bridge.calls.find((call) => call.channel === "chat.conversations.create")?.payload,
  ).not.toHaveProperty("enabledTools");
  events.dispatch({ type: "end", ...handle, seq: 1, ts: Date.now(), outcome: { status: "ok" } });
  await vi.waitFor(() => expect(store.generating).toBe(false));

  store.typeText("/browser-use");
  await vi.waitFor(() => expect(store.canSend).toBe(true));
  await store.send();
  expect(store.composer.commands).toEqual(["browser-use"]);
  expect(bridge.calls.filter((call) => call.channel === "chat.send")).toHaveLength(1);

  store.removeCommand("browser-use");
  store.typeText("/browser-use /goal  Следующая страница ");
  await store.send();
  expect(bridge.calls.filter((call) => call.channel === "chat.send").at(-1)?.payload).toMatchObject(
    { text: "Следующая страница", commands: ["browser-use", "goal"], plugins: [] },
  );
  events.dispatch({ type: "end", ...handle, seq: 2, ts: Date.now(), outcome: { status: "ok" } });
  await vi.waitFor(() => expect(store.generating).toBe(false));
  store.dispose();
  events.dispose();
});

test("commands match the slash token at the caret and parse only when known", () => {
  expect(slashQuery("/BR")).toBe("br");
  expect(slashQuery("br")).toBeNull();
  expect(slashQuery("/br x")).toBeNull();
  expect(slashQuery("Найди /gi")).toBe("gi");
  expect(slashQuery("a/b")).toBeNull();
  expect(withoutSlashToken("Найди /gi")).toBe("Найди ");
  expect(
    menuEntries({
      commands: [browserUse, goal],
      accounts: [work, off],
      attachedCommands: ["goal"],
      attachedPlugins: [],
      text: "/",
      picking: null,
    }).map((entry) => [entry.label, entry.description]),
  ).toEqual([
    ["/browser-use", browserUse.description],
    ["/github", "Файлы, задачи и запросы на слияние · Работа"],
  ]);
  expect(parseCommands([browserUse, goal], "  /Browser-Use  найди\nрасписание ")).toEqual({
    commands: ["browser-use"],
    text: "найди\nрасписание",
  });
  expect(parseCommands([browserUse, goal], "/goal /browser-use")).toEqual({
    commands: ["goal", "browser-use"],
    text: "",
  });
  expect(parseCommands([browserUse], "/deploy now")).toBeNull();
  expect(parseCommands([browserUse], "no slash")).toBeNull();
  expect(commandText("browser-use")).toBe("/browser-use");
});

test("tool sections group the offered tools by why they were offered", () => {
  const items: ChatContextToolDto[] = [
    { key: "docs.search", tokens: 1, source: "loaded" },
    { key: "fs.read", tokens: 1, source: "base" },
    { key: "chat.memory", tokens: 1, source: "memory" },
    { key: "browser.navigate", tokens: 1, source: "command" },
    { key: "knowledge.search", tokens: 1, source: "knowledge" },
    { key: "agent.delegate", tokens: 1, source: "subagent" },
  ];
  expect(toolSections(items)).toEqual([
    { title: BASE_TOOLS_TITLE, items: [items[1]] },
    { title: AUTOMATIC_TOOLS_TITLE, items: [items[2], items[4], items[5]] },
    { title: COMMAND_TOOLS_TITLE, items: [items[3]] },
    { title: LOADED_TOOLS_TITLE, items: [items[0]] },
  ]);
  expect(toolSections([])).toEqual([]);
  expect(SOURCE_HINTS).toEqual({
    base: null,
    knowledge: OFFERED_BY_KNOWLEDGE,
    memory: OFFERED_BY_MEMORY,
    subagent: OFFERED_BY_SUBAGENTS,
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
    { text: "Поиск.", reasoning: "Мысль.", estimated: false },
    { text: "\n\nИтог.", reasoning: "Ещё.", estimated: false },
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

test("a browser.navigate call opens the browser panel once per call", async () => {
  const { store, events } = setup();
  await store.mount();
  store.composer.setText("Question");
  await store.send();
  const navigate = ChatToolCallDto.parse({
    id: "call_nav",
    key: BROWSER_NAVIGATE_TOOL,
    name: "browser_navigate",
    round: 0,
    arguments: { url: "https://example.com" },
    status: "running",
  });
  const step = (seq: number, index: number, call: ChatToolCallDto) =>
    events.dispatch({
      type: "step",
      ...handle,
      seq,
      ts: Date.now(),
      step: { domain: "tool-call", index, call },
    });

  step(1, 0, { ...pendingCall, status: "running" });
  expect(store.isPanelShown("browser")).toBe(false);

  step(2, 1, navigate);
  expect(store.isPanelShown("browser")).toBe(true);

  store.closePanel();
  step(3, 1, { ...navigate, status: "succeeded" });
  expect(store.isPanelShown("browser")).toBe(false);

  step(4, 2, { ...navigate, id: "call_nav_2" });
  expect(store.isPanelShown("browser")).toBe(true);
  store.dispose();
  events.dispose();
});

test("commands carry their own menu marks", () => {
  expect(commandMark("browser-use")).toBe(mdiWeb);
  expect(commandMark("goal")).toBe(mdiFlagOutline);
});

test("the command menu shows command marks and plugin marks", async () => {
  const { store, events } = setup();
  await store.mount();
  store.typeText("/");
  render(<ChatCommandMenu store={store} />);
  const browserOption = screen.getByRole("option", { name: /\/browser-use/u });
  expect(browserOption.querySelector(`svg path[d="${mdiWeb}"]`)).not.toBeNull();
  const goalOption = screen.getByRole("option", { name: /\/goal/u });
  expect(goalOption.querySelector(`svg path[d="${mdiFlagOutline}"]`)).not.toBeNull();
  const telegramOption = screen.getByRole("option", { name: /\/telegram/u });
  expect(
    telegramOption.querySelector(`svg path[d="${PLUGIN_MARKS.telegram.logo}"]`),
  ).not.toBeNull();
  store.dispose();
  events.dispose();
});

test("pluginToolKind reads the integration prefix of account tool keys", () => {
  expect(pluginToolKind("telegram.135acee8.recentMessages")).toBe("telegram");
  expect(pluginToolKind("mail.deadbeef.send.find")).toBe("mail");
  expect(pluginToolKind("fs.read")).toBeNull();
  expect(pluginToolKind("mcp.docs.search")).toBeNull();
  expect(pluginToolKind("unknown.12345678.call")).toBeNull();
});

test("a plugin tool call carries the plugin mark instead of the generic icon", () => {
  const telegramCall = ChatToolCallDto.parse({
    id: "call_tg",
    key: "telegram.135acee8.sendMessage",
    name: "telegram_135acee8_sendMessage",
    round: 0,
    arguments: { chatId: "1", text: "привет" },
    status: "succeeded",
  });
  const { container } = render(<ChatToolCalls calls={[telegramCall]} />);
  expect(container.querySelector(`svg path[d="${PLUGIN_MARKS.telegram.logo}"]`)).not.toBeNull();
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
