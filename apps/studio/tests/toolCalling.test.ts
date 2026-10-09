import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "vitest";
import { REPO_ROOT } from "../../../test/helpers/paths.ts";
import { temporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { NodeRegistry } from "../src/host/kernel/NodeRegistry.ts";
import { registerCoreNodes } from "../src/host/kernel/coreNodes.ts";
import { registerKernelTools } from "../src/host/tools/kernelTools.ts";
import { ToolRegistry } from "../src/host/tools/ToolRegistry.ts";
import { createFakeDriver } from "../../../test/helpers/FakeDriver.ts";
import { createFakeTransport } from "../../../test/helpers/FakeTransport.ts";
import { OpenAiCompatibleAdapter } from "../src/host/drivers/ai/adapters/openaiCompatible.ts";
import { OllamaAdapter } from "../src/host/drivers/ai/adapters/ollama.ts";
import {
  QWEN_CHATS_NEW_PATH,
  QWEN_COMPLETIONS_PATH,
  QWEN_MODELS_PATH,
  QwenWebAdapter,
} from "../src/host/drivers/ai/adapters/qwenWeb.ts";
import {
  INJECTED_CALL_KEY,
  INJECTED_RESULT_PREFIX,
  InjectedCallBuffer,
  couldBeInjectedCall,
  injectedRequest,
  parseInjectedCall,
  withInjectedTools,
} from "../src/host/drivers/ai/injectedTools.ts";
import type { GenerateRequest, TextDelta, ToolSpec } from "../src/host/drivers/ai/ports.ts";
import {
  offeredTools,
  toolParameters,
  toolResultBudget,
  wireToolName,
} from "../src/host/services/chatTools.ts";
import { modelToolSupport, resolveToolMode, ToolDto } from "@zvs/shared";

const fixtures = join(REPO_ROOT, "test", "fixtures", "providers");
const fixtureText = (name: string) => readFileSync(join(fixtures, name), "utf8");
const frames = (name: string) =>
  fixtureText(name)
    .split("\n\n")
    .filter((frame) => frame.trim().length > 0)
    .map((frame) => `${frame}\n\n`);

const search: ToolSpec = {
  name: "docs__search",
  description: "Search the docs",
  parameters: { type: "object", properties: { query: { type: "string" } }, required: ["query"] },
};

const request: GenerateRequest = {
  model: "openai/gpt-4o-mini",
  system: "Be brief.",
  tools: [search],
  messages: [
    { role: "user", content: "How do I rotate a token?" },
    {
      role: "assistant",
      content: "",
      toolCalls: [{ id: "call_1", name: "docs__search", arguments: '{"query":"rotate"}' }],
    },
    { role: "tool", toolCallId: "call_1", content: "Use the CLI." },
  ],
};

async function drain(deltas: AsyncIterable<TextDelta>): Promise<TextDelta[]> {
  const collected: TextDelta[] = [];
  for await (const delta of deltas) collected.push(delta);
  return collected;
}

test("openai-compatible maps tools and tool messages onto the wire and assembles streamed calls", async () => {
  const transport = createFakeTransport();
  transport.reply("/chat/completions", { chunks: frames("openrouterToolCalls.sse") });
  const adapter = new OpenAiCompatibleAdapter({ transport });

  const deltas = await drain(adapter.stream(request, new AbortController().signal));

  expect(deltas).toEqual([
    { kind: "text", text: "Сейчас проверю." },
    {
      kind: "tool_call",
      call: { id: "call_Rk8Yv2", name: "docs__search", arguments: '{"query":"rotate token"}' },
    },
    { kind: "tool_call", call: { id: "call_P0q1Lm", name: "clock__now", arguments: "{}" } },
  ]);
  const body = transport.lastBody<Record<string, unknown>>();
  expect(body.tools).toEqual([
    {
      type: "function",
      function: {
        name: "docs__search",
        description: "Search the docs",
        parameters: search.parameters,
      },
    },
  ]);
  expect(body.messages).toEqual([
    { role: "system", content: "Be brief." },
    { role: "user", content: "How do I rotate a token?" },
    {
      role: "assistant",
      content: null,
      tool_calls: [
        {
          id: "call_1",
          type: "function",
          function: { name: "docs__search", arguments: '{"query":"rotate"}' },
        },
      ],
    },
    { role: "tool", tool_call_id: "call_1", content: "Use the CLI." },
  ]);
});

test("openai-compatible reads tool calls from a non-streamed completion", async () => {
  const transport = createFakeTransport();
  transport.reply("/chat/completions", {
    body: {
      model: "openai/gpt-4o-mini",
      choices: [
        {
          finish_reason: "tool_calls",
          message: {
            content: null,
            tool_calls: [
              {
                id: "call_9",
                type: "function",
                function: { name: "docs__search", arguments: '{"query":"x"}' },
              },
            ],
          },
        },
      ],
    },
  });
  const result = await new OpenAiCompatibleAdapter({ transport }).generate(
    { model: "m", messages: [{ role: "user", content: "x" }], tools: [search] },
    new AbortController().signal,
  );
  expect(result.finishReason).toBe("tool_calls");
  expect(result.toolCalls).toEqual([
    { id: "call_9", name: "docs__search", arguments: '{"query":"x"}' },
  ]);
});

test("a request without tools sends no tools field", async () => {
  const transport = createFakeTransport();
  transport.reply("/chat/completions", { chunks: [] });
  await drain(
    new OpenAiCompatibleAdapter({ transport }).stream(
      { model: "m", messages: [{ role: "user", content: "x" }] },
      new AbortController().signal,
    ),
  );
  expect("tools" in transport.lastBody<Record<string, unknown>>()).toBe(false);
});

test("Ollama maps tools natively and turns object arguments into a JSON string", async () => {
  const transport = createFakeTransport("http://localhost:11434/api");
  transport.reply("/chat", { chunks: [fixtureText("ollamaToolCalls.ndjson")] });
  const deltas = await drain(
    new OllamaAdapter(transport).stream(request, new AbortController().signal),
  );
  expect(deltas).toEqual([
    {
      kind: "tool_call",
      call: { id: "call_1", name: "docs__search", arguments: '{"query":"rotate token","k":3}' },
    },
  ]);
  const body = transport.lastBody<{ messages: unknown[]; tools: unknown[] }>();
  expect(body.tools).toHaveLength(1);
  expect(body.messages[2]).toEqual({
    role: "assistant",
    content: "",
    tool_calls: [{ function: { name: "docs__search", arguments: { query: "rotate" } } }],
  });
  expect(body.messages[3]).toEqual({ role: "tool", content: "Use the CLI." });
});

test("the injected parser accepts one marked object, fenced or bare, and nothing else", () => {
  const names = new Set(["docs__search"]);
  expect(
    parseInjectedCall(
      `{"${INJECTED_CALL_KEY}": "docs__search", "arguments": {"query": "a"}}`,
      names,
    ),
  ).toEqual({ type: "call", name: "docs__search", arguments: '{"query":"a"}' });
  expect(
    parseInjectedCall(
      `\`\`\`json\n{"${INJECTED_CALL_KEY}": "docs__search", "arguments": {}}\n\`\`\``,
      names,
    ),
  ).toEqual({ type: "call", name: "docs__search", arguments: "{}" });
  expect(parseInjectedCall(`{"${INJECTED_CALL_KEY}": "docs__search"}`, names)).toMatchObject({
    type: "call",
    arguments: "{}",
  });
  for (const text of [
    "Plain answer.",
    `{"${INJECTED_CALL_KEY}": "docs__search", "arguments": {"query": "a"}`,
    `{"${INJECTED_CALL_KEY}": "docs__search", "arguments": {"query": }}`,
    `{"name": "docs__search", "arguments": {}}`,
    `{"${INJECTED_CALL_KEY}": "rm_rf", "arguments": {}}`,
    `{"${INJECTED_CALL_KEY}": "docs__search", "arguments": ["a"]}`,
    `Sure: {"${INJECTED_CALL_KEY}": "docs__search", "arguments": {}}`,
    `{"${INJECTED_CALL_KEY}": "docs__search", "arguments": {}} and more`,
    "[1, 2]",
  ])
    expect(parseInjectedCall(text, names), text).toEqual({ type: "text" });
});

test("a streamed prefix is held only while it can still become a call", () => {
  for (const prefix of [
    "",
    "  ",
    "`",
    "``",
    "```",
    "```j",
    "```json\n",
    "{",
    '{ "zvs_c',
    `{"${INJECTED_CALL_KEY}": "x`,
  ])
    expect(couldBeInjectedCall(prefix), prefix).toBe(true);
  for (const prefix of ["H", "Hello {", "```python", '{"name"', '{"zvs_x', "[", "``x"])
    expect(couldBeInjectedCall(prefix), prefix).toBe(false);

  const plain = new InjectedCallBuffer();
  expect(plain.push("Hel")).toBe("Hel");
  expect(plain.push("lo")).toBe("lo");
  expect(plain.finish()).toEqual({ type: "text", rest: "" });

  const json = new InjectedCallBuffer();
  expect(json.push('{"name": 1')).toBe('{"name": 1');

  const call = new InjectedCallBuffer();
  expect(call.push(`{"${INJECTED_CALL_KEY}": "docs__se`)).toBe("");
  expect(call.push('arch", "arguments": {}}')).toBe("");
  expect(call.finish(new Set(["docs__search"]))).toMatchObject({ type: "call", rest: "" });

  const partial = new InjectedCallBuffer();
  expect(partial.push(`{"${INJECTED_CALL_KEY}": "docs__search", "argu`)).toBe("");
  expect(partial.finish(new Set(["docs__search"]))).toEqual({
    type: "text",
    rest: `{"${INJECTED_CALL_KEY}": "docs__search", "argu`,
  });
});

test("the injected request describes the tools in the system prompt and fences results", () => {
  const injected = injectedRequest(request);
  expect(injected.tools).toBeUndefined();
  expect(injected.system).toContain("Be brief.");
  expect(injected.system).toContain("Name: docs__search");
  expect(injected.system).toContain(`"${INJECTED_CALL_KEY}"`);
  expect(injected.messages[1]).toEqual({
    role: "assistant",
    content: `{"${INJECTED_CALL_KEY}":"docs__search","arguments":{"query":"rotate"}}`,
  });
  expect(injected.messages[2]).toEqual({
    role: "user",
    content: `${INJECTED_RESULT_PREFIX} docs__search\n\`\`\`\nUse the CLI.\n\`\`\``,
  });
  expect(
    injectedRequest({
      ...request,
      messages: [{ role: "tool", toolCallId: "z", content: "```x```" }],
    }).messages[0]!.content,
  ).not.toMatch(/```x```/);
});

test("Qwen web calls a tool through the injected protocol from a recorded stream", async () => {
  const transport = createFakeTransport("https://chat.qwen.ai/api/v2");
  transport.reply(QWEN_MODELS_PATH, { text: fixtureText("qwenModels.json") });
  transport.reply(QWEN_CHATS_NEW_PATH, { text: fixtureText("qwenChatCreated.json") });
  transport.reply(QWEN_COMPLETIONS_PATH, { chunks: frames("qwenToolCall.sse") });
  const driver = withInjectedTools(new QwenWebAdapter({ transport }), () => "call_q1");

  const deltas = await drain(
    driver.stream(
      { model: "qwen3.7-plus", messages: [{ role: "user", content: "rotate?" }], tools: [search] },
      new AbortController().signal,
    ),
  );

  expect(deltas).toEqual([
    {
      kind: "tool_call",
      call: { id: "call_q1", name: "docs__search", arguments: '{"query":"rotate token"}' },
    },
  ]);
  const sent = JSON.stringify(transport.lastBody());
  expect(sent).toContain("AVAILABLE TOOLS");
  expect(sent).toContain(INJECTED_CALL_KEY);
});

test("the injected wrapper passes plain answers through as soon as they cannot be a call", async () => {
  const fake = createFakeDriver({ deltas: ["The ", "answer."] });
  const deltas = await drain(
    withInjectedTools(fake.text).stream(
      { model: "m", messages: [{ role: "user", content: "x" }], tools: [search] },
      new AbortController().signal,
    ),
  );
  expect(deltas).toEqual([
    { kind: "text", text: "The " },
    { kind: "text", text: "answer." },
  ]);
  expect(fake.requests[0]!.system).toContain("Name: docs__search");

  const generated = await withInjectedTools(
    createFakeDriver({ text: `{"${INJECTED_CALL_KEY}":"docs__search","arguments":{"query":"q"}}` })
      .text,
    () => "call_g",
  ).generate(
    { model: "m", messages: [{ role: "user", content: "x" }], tools: [search] },
    new AbortController().signal,
  );
  expect(generated).toMatchObject({
    text: "",
    finishReason: "tool_calls",
    toolCalls: [{ id: "call_g", name: "docs__search", arguments: '{"query":"q"}' }],
  });
});

test("tool names are wire-safe and unique, and only offerable tools are offered", () => {
  const taken = new Set<string>();
  const first = wireToolName("mcp.docs.search", taken);
  taken.add(first);
  expect(first).toBe("mcp__docs__search");
  expect(wireToolName("mcp.docs.search", taken)).toBe("mcp__docs__search_2");
  expect(wireToolName(`mcp.${"x".repeat(80)}`, new Set())).toHaveLength(64);
  expect(toolParameters(null)).toEqual({ type: "object", properties: {} });
  expect(
    toolParameters({ $schema: "https://json-schema.org/draft/2020-12/schema", properties: {} }),
  ).toEqual({ properties: {}, type: "object" });
  expect(toolResultBudget(-5)).toBe(500);
  expect(toolResultBudget(1_000_000)).toBe(8000);

  const tool = (key: string, extra: Partial<ToolDto> = {}): ToolDto =>
    ToolDto.parse({
      id: "0199bb11-4444-7111-8111-00000000000" + String(key.length % 10),
      key,
      source: "mcp",
      sourceRef: null,
      displayName: key,
      description: "",
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
  const offered = offeredTools(
    ["mcp.a.on", "mcp.a.off", "mcp.a.gone", "mcp.a.disabled"],
    [
      tool("mcp.a.on"),
      tool("mcp.a.off", { tier: "off" }),
      tool("mcp.a.gone", { available: false }),
      tool("mcp.a.disabled", { enabled: false }),
      tool("mcp.a.unselected"),
    ],
  );
  expect(offered.map((entry) => entry.key)).toEqual(["mcp.a.on"]);
});

test("a native adapter falls back to none only for models the provider reports as tool-less", () => {
  const withTools = { capabilities: ["streaming", "tools"] };
  const without = { capabilities: ["streaming"] };
  expect(modelToolSupport(withTools, [withTools, without])).toBe(true);
  expect(modelToolSupport(without, [withTools, without])).toBe(false);
  expect(modelToolSupport(without, [without])).toBeNull();
  expect(resolveToolMode("native", false)).toBe("none");
  expect(resolveToolMode("native", null)).toBe("native");
  expect(resolveToolMode("injected", false)).toBe("injected");
  expect(resolveToolMode("none", true)).toBe("none");
});

test("vector.list is a read-only tool that tells the model which UUID to pass to vector.search", async () => {
  const database = temporaryDatabase();
  try {
    const nodes = registerCoreNodes(new NodeRegistry());
    const registry = new ToolRegistry({ data: database.client });
    const registered = registerKernelTools(nodes, registry);
    const list = registered.find((entry) => entry.key === "vector.list");
    expect(list).toMatchObject({ readOnly: true, tier: "auto" });
    const search = registered.find((entry) => entry.key === "vector.search")!;
    expect(JSON.stringify(search.inputSchema)).toContain("as returned by vector.list");
    expect(search.description).toContain("vector.list");

    const store = {
      id: "0199bb11-7777-7111-8111-000000000001",
      name: "Manuals",
      description: "Product manuals",
      documents: 12,
      status: "healthy",
      searchMode: "hybrid",
      vectors: 300,
    };
    const output = await nodes.get("vector.list").execute(
      {
        runId: "0199bb11-7777-7111-8111-000000000002" as never,
        signal: new AbortController().signal,
        emit: () => undefined,
        requestApproval: async () => "denied",
        services: {
          vectorStores: { search: async () => [], list: async () => [store] as never },
        },
      },
      {},
    );
    expect(output).toEqual([
      {
        id: store.id,
        name: "Manuals",
        description: "Product manuals",
        documents: 12,
        status: "healthy",
        searchMode: "hybrid",
      },
    ]);
  } finally {
    database.dispose();
  }
});
