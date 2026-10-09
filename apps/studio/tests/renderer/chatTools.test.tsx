import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  ChatToolCallDto,
  ConversationDetailDto,
  RunHandleDto,
  ToolDto,
  contract,
  type Contract,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import ChatToolCalls from "../../src/renderer/features/chat/ChatToolCalls";
import { toolBlockReason } from "../../src/renderer/features/chat/ChatToolPicker";
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

function setup() {
  let detail = structuredClone(conversation);
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [detail])
    .handle("chat.conversations.get", () => detail)
    .handle("chat.conversations.create", (input) => {
      detail = { ...detail, enabledTools: input.enabledTools ?? [] };
      return detail;
    })
    .handle("chat.conversations.updateTools", (input) => {
      detail = { ...detail, enabledTools: input.enabledTools };
      return detail;
    })
    .handle("providers.list", () => [saved])
    .handle("providers.get", () => saved)
    .handle("vectorStores.list", () => [])
    .handle("tools.list", () => [tool("mcp.docs.search"), tool("mcp.docs.drop", { tier: "off" })])
    .handle("chat.send", () => handle)
    .handle("runs.approve", () => undefined);
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new ChatStore(createIpcClient(contract, bridge), events);
  return { store, bridge, events };
}

test("the conversation's tool picks travel with create and later with updateTools", async () => {
  const { store, bridge, events } = setup();
  await store.mount();
  expect(store.tools.map((entry) => entry.key)).toEqual(["mcp.docs.search", "mcp.docs.drop"]);
  expect(store.toolMode).toBe("native");

  await store.toggleTool("mcp.docs.search");
  expect(store.enabledTools).toEqual(["mcp.docs.search"]);
  expect(store.offeredToolCount).toBe(1);
  store.composer.setText("Question");
  await store.send();
  expect(
    bridge.calls.find((call) => call.channel === "chat.conversations.create")?.payload,
  ).toMatchObject({ enabledTools: ["mcp.docs.search"] });

  events.dispatch({ type: "end", ...handle, seq: 1, ts: Date.now(), outcome: { status: "ok" } });
  await vi.waitFor(() => expect(store.generating).toBe(false));
  await store.toggleTool("mcp.docs.search");
  expect(bridge.calls.at(-1)).toMatchObject({
    channel: "chat.conversations.updateTools",
    payload: { id: conversation.id, enabledTools: [] },
  });
  expect(store.enabledTools).toEqual([]);
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
    step: { domain: "tool-calls", calls: [pendingCall] },
  });
  events.dispatch({
    type: "approval",
    ...handle,
    seq: 2,
    ts: Date.now(),
    request: { id: approvalId, runId: handle.id, subject: "mcp.docs.search" },
  });
  events.dispatch({
    type: "approval",
    ...handle,
    seq: 3,
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
  expect(screen.getByText("1.5 с")).toBeTruthy();
});

test("a tool that cannot be offered explains why", () => {
  expect(toolBlockReason(tool("mcp.a.b"))).toBeNull();
  expect(toolBlockReason(tool("mcp.a.b", { tier: "off" }))).toMatch(/Запрещён/);
  expect(toolBlockReason(tool("mcp.a.b", { enabled: false }))).toMatch(/Выключен/);
  expect(toolBlockReason(tool("mcp.a.b", { available: false }))).toMatch(/недоступен/);
});
