import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { z } from "zod";
import {
  AppErrorCode,
  CreateConversationInput,
  HostEvent,
  type ChatToolCallDto,
  type ToolDto,
} from "@zvs/shared";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { FAKE_CAPABILITIES } from "../../../test/helpers/FakeDriver.ts";
import { NodeRegistry } from "../src/host/kernel/NodeRegistry.ts";
import { registerCoreNodes } from "../src/host/kernel/coreNodes.ts";
import { createEventBus, type EventBus } from "../src/host/platform/events.ts";
import { RunService } from "../src/host/services/RunService.ts";
import { ChatService } from "../src/host/services/ChatService.ts";
import { ToolRegistry } from "../src/host/tools/ToolRegistry.ts";
import { INJECTED_CALL_KEY, INJECTED_RESULT_PREFIX } from "../src/host/drivers/ai/injectedTools.ts";
import type {
  GenerateRequest,
  TextDelta,
  TextGenerationDriver,
} from "../src/host/drivers/ai/ports.ts";
import type { ToolMode } from "@zvs/shared";
import {
  CHAT_TOOL_CALLS,
  CHAT_TOOL_ROUNDS,
  TOOL_BUDGET_NOTE,
  TOOL_RESULTS_INSTRUCTION,
} from "../src/host/services/chatTools.ts";

type Round = (request: GenerateRequest, signal: AbortSignal) => TextDelta[] | Promise<TextDelta[]>;

let db: TemporaryDatabase;
let bus: EventBus;
let runs: RunService;
let chat: ChatService;
let tools: ToolRegistry;
let events: HostEvent[];
let providerId: string;
let modelId: string;
let requests: GenerateRequest[];
let rounds: Round[];
let fallback: Round;
let mode: ToolMode;
let lookup: ReturnType<typeof vi.fn<(query: string, signal: AbortSignal) => Promise<string>>>;
let drop: ReturnType<typeof vi.fn<() => Promise<string>>>;
let lookupTool: ToolDto;
let dropTool: ToolDto;

const text = (value: string): TextDelta => ({ kind: "text", text: value });
const call = (name: string, args: unknown, id = `call_${name}`): TextDelta => ({
  kind: "tool_call",
  call: { id, name, arguments: JSON.stringify(args) },
});

const driver: TextGenerationDriver = {
  capabilities: () => ({ ...FAKE_CAPABILITIES, tools: mode }),
  listModels: async () => [],
  generate: () => Promise.reject(new Error("not used")),
  stream(request, signal) {
    requests.push(structuredClone(request));
    const round = rounds.shift() ?? fallback;
    return {
      async *[Symbol.asyncIterator]() {
        for (const delta of await round(request, signal)) {
          signal.throwIfAborted();
          yield delta;
        }
      },
    };
  },
};

beforeEach(() => {
  db = temporaryDatabase();
  events = [];
  requests = [];
  rounds = [];
  fallback = () => [text("Done.")];
  mode = "native";
  const registry = registerCoreNodes(new NodeRegistry());
  lookup = vi.fn(async (query: string) => `found ${query}`);
  drop = vi.fn(async () => "dropped");
  registry.register({
    type: "demo.lookup",
    input: z.object({ query: z.string() }),
    output: z.object({ text: z.string() }),
    permission: { tool: "demo.lookup" },
    sideEffectFree: true,
    run: async (context, input) => ({ text: await lookup(input.query, context.signal) }),
  });
  registry.register({
    type: "demo.drop",
    input: z.object({}).loose(),
    output: z.object({ text: z.string() }),
    permission: { tool: "demo.drop" },
    sideEffect: true,
    run: async () => ({ text: await drop() }),
  });
  bus = createEventBus({
    senders: () => [
      { isDestroyed: () => false, send: (_channel, event) => events.push(HostEvent.parse(event)) },
    ],
  });
  tools = new ToolRegistry({ data: db.client });
  lookupTool = tools.register({
    key: "demo.lookup",
    source: "builtin",
    description: "Look a fact up",
    inputSchema: { type: "object", properties: { query: { type: "string" } } },
    readOnly: true,
  });
  dropTool = tools.register({ key: "demo.drop", source: "builtin", destructive: true });
  runs = new RunService({
    data: db.client,
    registry,
    events: bus,
    tools,
    services: {
      providers: {
        text: async () => driver,
        ephemeralDriver: async () => ({ text: driver, embedding: null, image: null }),
      },
    },
  });
  chat = new ChatService({ data: db.client, registry, runs, tools });
  providerId = db.client.repositories.providers.create({
    name: "Fake",
    kind: "ollama",
    baseUrl: "http://localhost:11434",
    capabilities: ["text"],
    createdAt: 1,
    updatedAt: 1,
  }).id;
  modelId = db.client.repositories.models.replaceForProvider(providerId, [
    {
      externalId: "fake-model",
      displayName: "Fake",
      contextWindow: 16384,
      maxOutput: 512,
      discoveredAt: 1,
    },
  ])[0]!.id;
});

afterEach(async () => {
  await runs.dispose();
  bus.dispose();
  db.dispose();
});

function conversation(enabledTools: string[] = ["demo.lookup"]) {
  return chat.create(CreateConversationInput.parse({ providerId, modelId, enabledTools }));
}

async function turn(conversationId: string, message = "Question") {
  const handle = chat.sendMessage(conversationId, message);
  await runs.wait(handle.id);
  return { handle, assistant: chat.get(conversationId).messages.at(-1)! };
}

function toolSteps(runId: string) {
  return runs.steps(runId).filter((step) => step.nodeId.startsWith("generate."));
}

test("a native call is admitted, run as its own step and fed back fenced before the answer", async () => {
  rounds = [() => [text("Checking."), call("demo__lookup", { query: "x" })]];
  fallback = () => [text("It is x.")];
  const created = conversation();

  const { handle, assistant } = await turn(created.id);

  expect(runs.get(handle.id).status).toBe("succeeded");
  expect(lookup).toHaveBeenCalledTimes(1);
  expect(requests[0]!.tools).toEqual([
    {
      name: "demo__lookup",
      description: "Look a fact up",
      parameters: { type: "object", properties: { query: { type: "string" } } },
    },
  ]);
  expect(requests[0]!.system).toContain(TOOL_RESULTS_INSTRUCTION);
  expect(requests[1]!.messages.slice(-2)).toEqual([
    {
      role: "assistant",
      content: "Checking.",
      toolCalls: [{ id: "call_demo__lookup", name: "demo__lookup", arguments: '{"query":"x"}' }],
    },
    {
      role: "tool",
      toolCallId: "call_demo__lookup",
      content: '<tool_result name="demo__lookup" status="succeeded">\nfound x\n</tool_result>',
    },
  ]);
  expect(assistant).toMatchObject({ content: "Checking.\n\nIt is x.", partial: false });
  expect(assistant.toolCalls).toEqual([
    expect.objectContaining({
      key: "demo.lookup",
      name: "demo__lookup",
      status: "succeeded",
      arguments: { query: "x" },
      result: "found x",
      round: 0,
    }),
  ]);
  const steps = toolSteps(handle.id);
  expect(steps).toEqual([
    expect.objectContaining({ nodeId: "generate.call1", type: "demo.lookup", status: "succeeded" }),
  ]);
  expect(assistant.toolCalls[0]!.stepId).toBe(steps[0]!.id);
  expect(tools.get(lookupTool.id).callCount).toBe(1);
  expect(runs.list().items[0]!.progress).toEqual({ done: 3, total: 3 });
  expect(events.some((event) => event.type === "step" && event.step.domain === "tool-calls")).toBe(
    true,
  );
});

test("only tools picked for the conversation and not off are offered", async () => {
  await turn(conversation([]).id);
  expect(requests[0]!.tools).toBeUndefined();
  expect(requests[0]!.system ?? "").not.toContain(TOOL_RESULTS_INSTRUCTION);

  tools.setTier(lookupTool.id, "off");
  await turn(conversation().id);
  expect(requests[1]!.tools).toBeUndefined();
});

test("an off tool is refused at admission even when it was offered at the start of the turn", async () => {
  rounds = [
    () => {
      tools.setTier(lookupTool.id, "off");
      return [call("demo__lookup", { query: "x" })];
    },
  ];
  const created = conversation();

  const { handle, assistant } = await turn(created.id);

  expect(runs.get(handle.id).status).toBe("succeeded");
  expect(lookup).not.toHaveBeenCalled();
  expect(assistant.toolCalls[0]).toMatchObject({ status: "refused", result: null });
  expect(toolSteps(handle.id)[0]).toMatchObject({ status: "failed" });
  expect(requests[1]!.messages.at(-1)!.content).toContain("Tool call refused");
  expect(assistant.content).toBe("Done.");

  await turn(created.id);
  expect(requests[2]!.tools).toBeUndefined();
});

test("a destructive tool asks for approval even at tier auto, and runs only once approved", async () => {
  tools.setTier(dropTool.id, "auto");
  rounds = [() => [call("demo__drop", {})]];
  const created = conversation(["demo.drop"]);
  const handle = chat.sendMessage(created.id, "Drop it");

  let pending: ChatToolCallDto | undefined;
  await vi.waitFor(() => {
    pending = chat.get(created.id).messages.at(-1)!.toolCalls[0];
    expect(pending?.status).toBe("awaiting-approval");
  });
  expect(drop).not.toHaveBeenCalled();
  expect(runs.get(handle.id).status).toBe("blocked");
  const approval = events.find((event) => event.type === "approval");
  expect(approval?.type === "approval" && approval.request.id).toBe(pending!.approvalId);
  runs.approve(handle.id, pending!.approvalId!);
  await runs.wait(handle.id);

  expect(drop).toHaveBeenCalledTimes(1);
  expect(chat.get(created.id).messages.at(-1)!.toolCalls[0]).toMatchObject({
    status: "succeeded",
    result: "dropped",
  });
});

test("a denied approval is reported to the model and the turn still answers", async () => {
  tools.setTier(dropTool.id, "ask");
  rounds = [() => [call("demo__drop", {})]];
  const created = conversation(["demo.drop"]);
  const handle = chat.sendMessage(created.id, "Drop it");
  let approvalId: string | null = null;
  await vi.waitFor(() => {
    approvalId = chat.get(created.id).messages.at(-1)!.toolCalls[0]?.approvalId ?? null;
    expect(approvalId).not.toBeNull();
  });
  runs.deny(handle.id, approvalId!);
  await runs.wait(handle.id);

  expect(runs.get(handle.id).status).toBe("succeeded");
  expect(drop).not.toHaveBeenCalled();
  const assistant = chat.get(created.id).messages.at(-1)!;
  expect(assistant.toolCalls[0]).toMatchObject({ status: "denied" });
  expect(assistant.content).toBe("Done.");
  expect(requests[1]!.messages.at(-1)!.content).toContain("Tool call denied");
});

test("the loop stops at the round cap and answers without running more tools", async () => {
  fallback = () => [call("demo__lookup", { query: String(requests.length) })];
  const created = conversation();

  const { handle, assistant } = await turn(created.id);

  expect(runs.get(handle.id).status).toBe("succeeded");
  expect(lookup).toHaveBeenCalledTimes(CHAT_TOOL_ROUNDS);
  expect(requests).toHaveLength(CHAT_TOOL_ROUNDS + 1);
  expect(requests.at(-1)!.messages.some((message) => message.content === TOOL_BUDGET_NOTE)).toBe(
    true,
  );
  expect(assistant.toolCalls).toHaveLength(CHAT_TOOL_ROUNDS + 1);
  expect(assistant.toolCalls.at(-1)).toMatchObject({ status: "refused" });
  expect(toolSteps(handle.id)).toHaveLength(CHAT_TOOL_ROUNDS);
});

test("the loop stops at the call cap across rounds", async () => {
  fallback = () => [
    call("demo__lookup", { query: "a" }, `a${String(requests.length)}`),
    call("demo__lookup", { query: "b" }, `b${String(requests.length)}`),
    call("demo__lookup", { query: "c" }, `c${String(requests.length)}`),
  ];
  const { assistant } = await turn(conversation().id);

  expect(lookup).toHaveBeenCalledTimes(CHAT_TOOL_CALLS);
  expect(assistant.toolCalls.filter((entry) => entry.status === "succeeded")).toHaveLength(
    CHAT_TOOL_CALLS,
  );
  expect(requests.at(-1)!.messages.some((message) => message.content === TOOL_BUDGET_NOTE)).toBe(
    true,
  );
});

test("cancelling during a tool call leaves a consistent, partial conversation", async () => {
  let started = false;
  lookup.mockImplementation(
    (_query, signal) =>
      new Promise((_resolve, reject) => {
        started = true;
        signal.addEventListener("abort", () => reject(new Error("aborted")), { once: true });
      }),
  );
  rounds = [() => [text("Let me look."), call("demo__lookup", { query: "x" })]];
  const created = conversation();
  const handle = chat.sendMessage(created.id, "Question");
  await vi.waitFor(() => expect(started).toBe(true));

  await chat.cancel(handle.id);

  expect(runs.get(handle.id).status).toBe("cancelled");
  const assistant = chat.get(created.id).messages.at(-1)!;
  expect(assistant).toMatchObject({ role: "assistant", partial: true, content: "Let me look." });
  expect(assistant.toolCalls).toEqual([
    expect.objectContaining({ status: "cancelled", error: "Выполнение отменено" }),
  ]);
  expect(toolSteps(handle.id)[0]).toMatchObject({ status: "cancelled" });

  lookup.mockImplementation(async (query) => `found ${query}`);
  const next = await turn(created.id, "Again");
  expect(runs.get(next.handle.id).status).toBe("succeeded");
  expect(requests.at(-1)!.messages.every((message) => message.role !== "tool")).toBe(true);
  expect(requests.at(-1)!.messages.every((message) => message.toolCalls === undefined)).toBe(true);
});

test("a call to a tool that was not offered is refused without a step", async () => {
  rounds = [() => [call("demo__drop", {})]];
  const { handle, assistant } = await turn(conversation().id);
  expect(drop).not.toHaveBeenCalled();
  expect(assistant.toolCalls[0]).toMatchObject({ key: "demo__drop", status: "refused" });
  expect(toolSteps(handle.id)).toHaveLength(0);
});

test("malformed arguments fail the call without running the tool", async () => {
  rounds = [
    () => [{ kind: "tool_call", call: { id: "bad", name: "demo__lookup", arguments: "{oops" } }],
  ];
  const { assistant } = await turn(conversation().id);
  expect(lookup).not.toHaveBeenCalled();
  expect(assistant.toolCalls[0]).toMatchObject({ status: "failed", arguments: "{oops" });
});

test("an injected adapter calls the tool from a JSON reply that never reaches the user", async () => {
  mode = "injected";
  rounds = [
    () => [text(`{"${INJECTED_CALL_KEY}": "demo__`), text('lookup", "arguments": {"query": "y"}}')],
  ];
  fallback = () => [text("It is y.")];
  const created = conversation();

  const { handle, assistant } = await turn(created.id);

  expect(lookup).toHaveBeenCalledWith("y", expect.anything());
  expect(requests[0]!.tools).toBeUndefined();
  expect(requests[0]!.system).toContain("AVAILABLE TOOLS");
  expect(requests[1]!.messages.at(-1)).toEqual({
    role: "user",
    content: `${INJECTED_RESULT_PREFIX} demo__lookup\n\`\`\`\nfound y\n\`\`\``,
  });
  expect(assistant.content).toBe("It is y.");
  const tokens = events
    .filter((event) => event.type === "token" && event.streamId === handle.streamId)
    .map((event) => (event.type === "token" ? event.delta : ""))
    .join("");
  expect(tokens).toBe("It is y.");
});

test("a model the provider reports as tool-less is not offered tools", async () => {
  db.client.repositories.models.replaceForProvider(providerId, [
    {
      externalId: "fake-model",
      displayName: "Fake",
      contextWindow: 16384,
      maxOutput: 512,
      capabilities: ["streaming"],
      discoveredAt: 1,
    },
    {
      externalId: "tool-model",
      displayName: "Tools",
      contextWindow: 16384,
      capabilities: ["streaming", "tools"],
      discoveredAt: 1,
    },
  ]);
  const { handle } = await turn(conversation().id);
  expect(requests[0]!.tools).toBeUndefined();
  expect(runs.get(handle.id).status).toBe("succeeded");
});

test("conversation tool selection is validated and persisted", () => {
  const created = conversation([]);
  expect(chat.updateTools(created.id, ["demo.lookup", "demo.lookup"]).enabledTools).toEqual([
    "demo.lookup",
  ]);
  expect(chat.get(created.id).enabledTools).toEqual(["demo.lookup"]);
  expect(() => chat.updateTools(created.id, ["demo.missing"])).toThrow(
    expect.objectContaining({ code: AppErrorCode.NOT_FOUND }),
  );
  expect(() => conversation(["demo.missing"])).toThrow(
    expect.objectContaining({ code: AppErrorCode.NOT_FOUND }),
  );
});
