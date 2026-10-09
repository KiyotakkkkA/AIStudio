import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { z } from "zod";
import { AppErrorCode, contract, HostEvent, StartRunInput } from "@zvs/shared";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { NodeRegistry } from "../src/host/kernel/NodeRegistry.ts";
import { registerCoreNodes } from "../src/host/kernel/coreNodes.ts";
import { RunService } from "../src/host/services/RunService.ts";
import { createEventBus, type EventBus } from "../src/host/platform/events.ts";
import { createToolHandlers } from "../src/host/ipc/tools.ts";
import { defaultToolTier, ToolRegistry } from "../src/host/tools/ToolRegistry.ts";
import { registerKernelTools } from "../src/host/tools/kernelTools.ts";
import { ToolInvoker } from "../src/host/tools/ToolInvoker.ts";
import { SettingService } from "../src/host/services/SettingService.ts";

let db: TemporaryDatabase;
let nodes: NodeRegistry;
let tools: ToolRegistry;
let runs: RunService;
let bus: EventBus;
let events: HostEvent[];
let now = 1_700_000_000_000;
const execute = vi.fn(async () => "executed");

beforeEach(() => {
  db = temporaryDatabase();
  events = [];
  now = 1_700_000_000_000;
  execute.mockClear();
  nodes = new NodeRegistry();
  nodes.register({
    type: "tool",
    input: z.json(),
    output: z.string(),
    sideEffect: true,
    permission: { tool: "mail.send" },
    run: execute,
  });
  tools = new ToolRegistry({ data: db.client, clock: () => now });
  bus = createEventBus({
    senders: () => [
      { isDestroyed: () => false, send: (_channel, event) => events.push(HostEvent.parse(event)) },
    ],
  });
  runs = new RunService({
    data: db.client,
    registry: nodes,
    tools,
    events: bus,
    approvalTimeoutMs: 1000,
  });
});

afterEach(async () => {
  await runs.dispose();
  bus.dispose();
  db.dispose();
});

function descriptor(overrides: Record<string, unknown> = {}) {
  return {
    key: "mail.send",
    source: "builtin" as const,
    displayName: "mail.send",
    description: "Отправляет письмо",
    inputSchema: { type: "object" },
    readOnly: false,
    ...overrides,
  };
}

function start(scopes: string[] = []) {
  return runs.start(
    StartRunInput.parse({
      kind: "job",
      graph: { permissionScopes: scopes, nodes: [{ id: "a", type: "tool" }] },
    }),
  );
}

async function approvalId() {
  await vi.waitFor(() => expect(events.some((event) => event.type === "approval")).toBe(true));
  const event = events.find((event) => event.type === "approval");
  if (event?.type !== "approval") throw new Error("Missing approval");
  return String(event.request.id);
}

test.each([
  { label: "read-only", readOnly: true, destructive: false, expected: "auto" },
  { label: "side-effecting", readOnly: false, destructive: false, expected: "ask" },
  { label: "destructive", readOnly: false, destructive: true, expected: "off" },
  { label: "read-only but destructive", readOnly: true, destructive: true, expected: "off" },
])("a $label tool defaults to $expected", ({ readOnly, destructive, expected }) => {
  expect(defaultToolTier({ readOnly, destructive })).toBe(expected);
  expect(tools.register(descriptor({ readOnly, destructive })).tier).toBe(expected);
});

test("registration is idempotent by key and keeps the decided tier and usage", () => {
  const first = tools.register(descriptor());
  tools.recordUse("mail.send", now);
  tools.setTier(first.id, "auto");
  now += 1000;
  const second = tools.register(
    descriptor({ source: "mcp", sourceRef: "mail-server", description: "Обновлённое описание" }),
  );
  expect(second.id).toBe(first.id);
  expect(tools.list()).toHaveLength(1);
  expect(second.source).toBe("mcp");
  expect(second.sourceRef).toBe("mail-server");
  expect(second.description).toBe("Обновлённое описание");
  expect(second.tier).toBe("auto");
  expect(second.callCount).toBe(1);
  expect(second.registeredAt).toBe(first.registeredAt);
  expect(second.updatedAt).toBe(now);
});

test("a tool that becomes destructive is pushed back to off", () => {
  const registered = tools.register(descriptor({ readOnly: true }));
  expect(registered.tier).toBe("auto");
  expect(tools.register(descriptor({ readOnly: true, destructive: true })).tier).toBe("off");
});

test("the registered ceiling is the one admission composes against", () => {
  const registered = tools.register(descriptor({ readOnly: true }));
  expect(runs.permissions.admit("tool", { runId: "r", scopes: [] })).toBe("allow");
  tools.setTier(registered.id, "ask");
  expect(runs.permissions.admit("tool", { runId: "r", scopes: [] })).toBe("ask");
  expect(db.client.repositories.permissions.list("mail.send")).toHaveLength(1);
});

test("a narrowing scope cannot widen the registered ceiling", () => {
  const registered = tools.register(descriptor());
  tools.setTier(registered.id, "ask");
  runs.permissions.grant("mail.send", "skill:test", "auto", "user");
  expect(runs.permissions.admit("tool", { runId: "r", scopes: ["skill:test"] })).toBe("ask");
  tools.setTier(registered.id, "off");
  runs.permissions.grant("mail.send", "scenario:one", "auto", "user");
  expect(runs.permissions.admit("tool", { runId: "r", scopes: ["scenario:one"] })).toBe("deny");
  runs.permissions.grant("mail.send", "skill:test", "off", "user");
  tools.setTier(registered.id, "auto");
  expect(runs.permissions.admit("tool", { runId: "r", scopes: ["skill:test"] })).toBe("deny");
});

test("an always-allow grant cannot widen a registered ask ceiling", async () => {
  tools.register(descriptor());
  const handle = start(["skill:a"]);
  runs.approve(handle.id, await approvalId(), true);
  await runs.wait(handle.id);
  expect(execute).toHaveBeenCalledTimes(1);
  expect(runs.permissions.admit("tool", { runId: "r", scopes: ["skill:a"] })).toBe("ask");
});

test("disabling a tool blocks admission everywhere", async () => {
  const registered = tools.register(descriptor({ readOnly: true }));
  const allowed = start();
  await runs.wait(allowed.id);
  expect(runs.get(allowed.id).status).toBe("succeeded");
  tools.setEnabled(registered.id, false);
  const blocked = start();
  await runs.wait(blocked.id);
  expect(runs.get(blocked.id).outcome?.code).toBe(AppErrorCode.PERMISSION_DENIED);
  expect(execute).toHaveBeenCalledTimes(1);
  expect(runs.permissions.admit("tool", { runId: "r", scopes: [] })).toBe("deny");
});

test("setting a tool to off through the channel fails the run at admission", async () => {
  const registered = tools.register(descriptor({ readOnly: true }));
  createToolHandlers(tools)["tools.setTier"]({ id: registered.id, tier: "off" });
  const handle = start();
  await runs.wait(handle.id);
  expect(runs.get(handle.id).outcome?.code).toBe(AppErrorCode.PERMISSION_DENIED);
  expect(execute).not.toHaveBeenCalled();
});

test("chat stays permission free even when its capabilities are disabled", async () => {
  registerCoreNodes(nodes);
  const vector = registerKernelTools(nodes, tools).find((row) => row.key === "vector.search");
  expect(vector?.tier).toBe("auto");
  expect(vector?.readOnly).toBe(true);
  tools.setEnabled(vector!.id, false);
  const search = vi.fn(async () => []);
  const chat = new RunService({
    data: db.client,
    registry: nodes,
    tools,
    events: bus,
    services: { vectorStores: { search } },
  });
  try {
    const handle = chat.start(
      StartRunInput.parse({
        kind: "chat",
        graph: {
          nodes: [
            {
              id: "a",
              type: "vector.search",
              input: { storeId: crypto.randomUUID(), query: "вопрос" },
            },
          ],
        },
      }),
    );
    await chat.wait(handle.id);
    expect(chat.get(handle.id).status).toBe("succeeded");
    expect(search).toHaveBeenCalledTimes(1);
  } finally {
    await chat.dispose();
  }
});

test("a disabled tool is refused in the runs that do go through admission", async () => {
  registerCoreNodes(nodes);
  const vector = registerKernelTools(nodes, tools).find((row) => row.key === "vector.search");
  tools.setEnabled(vector!.id, false);
  const search = vi.fn(async () => []);
  const scenario = new RunService({
    data: db.client,
    registry: nodes,
    tools,
    events: bus,
    services: { vectorStores: { search } },
  });
  try {
    const handle = scenario.start(
      StartRunInput.parse({
        kind: "scenario",
        subjectId: "one",
        graph: {
          nodes: [
            {
              id: "a",
              type: "vector.search",
              input: { storeId: crypto.randomUUID(), query: "вопрос" },
            },
          ],
        },
      }),
    );
    await scenario.wait(handle.id);
    expect(scenario.get(handle.id).outcome?.code).toBe(AppErrorCode.PERMISSION_DENIED);
    expect(search).not.toHaveBeenCalled();
  } finally {
    await scenario.dispose();
  }
});

test("usage is recorded on every invocation and exposed through the detail rail", async () => {
  const registered = tools.register(descriptor({ readOnly: true }));
  await runs.wait(start().id);
  await runs.wait(start().id);
  const usage = tools.usage(registered.id);
  expect(usage.callCount).toBe(2);
  expect(usage.lastUsedAt).toBeGreaterThan(0);
  expect(tools.get(registered.id).callCount).toBe(2);
});

test("a denied invocation is not counted as a use", async () => {
  const registered = tools.register(descriptor());
  const handle = start();
  runs.deny(handle.id, await approvalId());
  await runs.wait(handle.id);
  expect(tools.usage(registered.id)).toMatchObject({ callCount: 0, lastUsedAt: null });
});

test("references are a replaceable reverse index over scenarios and skills", () => {
  const mail = tools.register(descriptor());
  const search = tools.register(
    descriptor({ key: "vector.search", displayName: "vector.search", readOnly: true }),
  );
  tools.recordReferences({ kind: "scenario", refId: "s1", name: "Триаж" }, [
    "mail.send",
    "vector.search",
    "does.notExist",
  ]);
  tools.recordReferences({ kind: "skill", refId: "k1", name: "Ответ клиенту" }, ["mail.send"]);
  expect(tools.get(mail.id).references).toEqual([
    { kind: "scenario", refId: "s1", name: "Триаж" },
    { kind: "skill", refId: "k1", name: "Ответ клиенту" },
  ]);
  tools.recordReferences({ kind: "scenario", refId: "s1", name: "Триаж" }, ["vector.search"]);
  expect(tools.get(mail.id).references).toEqual([
    { kind: "skill", refId: "k1", name: "Ответ клиенту" },
  ]);
  expect(tools.get(search.id).references).toEqual([
    { kind: "scenario", refId: "s1", name: "Триаж" },
  ]);
});

test("deregistering keeps the row, its history and its references, and denies admission", async () => {
  const registered = tools.register(descriptor({ readOnly: true }));
  await runs.wait(start().id);
  tools.recordReferences({ kind: "scenario", refId: "s1", name: "Триаж" }, ["mail.send"]);
  const gone = tools.deregister("mail.send");
  expect(gone?.available).toBe(false);
  expect(tools.list({ includeUnavailable: false })).toEqual([]);
  expect(tools.list()).toHaveLength(1);
  const usage = tools.usage(registered.id);
  expect(usage.callCount).toBe(1);
  expect(usage.references).toHaveLength(1);
  expect(runs.permissions.admit("tool", { runId: "r", scopes: [] })).toBe("deny");
  const blocked = start();
  await runs.wait(blocked.id);
  expect(runs.get(blocked.id).outcome?.code).toBe(AppErrorCode.PERMISSION_DENIED);
  expect(tools.register(descriptor({ readOnly: true })).available).toBe(true);
  expect(tools.usage(registered.id).callCount).toBe(1);
});

test("deregistering a source withdraws exactly that server's tools", () => {
  tools.register(descriptor({ key: "mail.send", source: "mcp", sourceRef: "server-a" }));
  tools.register(descriptor({ key: "mail.draft", source: "mcp", sourceRef: "server-a" }));
  tools.register(descriptor({ key: "fs.readFile", source: "mcp", sourceRef: "server-b" }));
  expect(tools.deregisterSource("mcp", "server-a").map((row) => row.key)).toEqual([
    "mail.draft",
    "mail.send",
  ]);
  expect(tools.list({ includeUnavailable: false }).map((row) => row.key)).toEqual(["fs.readFile"]);
});

test("the list filters by source, tier and free text", () => {
  tools.register(
    descriptor({ key: "vector.search", description: "Семантический поиск", readOnly: true }),
  );
  tools.register(descriptor({ key: "mail.send", source: "mcp", sourceRef: "server-a" }));
  expect(tools.list({ source: "mcp" }).map((row) => row.key)).toEqual(["mail.send"]);
  expect(tools.list({ tier: "auto" }).map((row) => row.key)).toEqual(["vector.search"]);
  expect(tools.list({ query: "ПИСЬМО" }).map((row) => row.key)).toEqual(["mail.send"]);
});

test("generation is not a tool and never reaches the registry", () => {
  registerCoreNodes(nodes);
  registerKernelTools(nodes, tools);
  expect(tools.list().map((row) => row.key)).not.toContain("llm.generate");
  expect(tools.ceiling("llm.generate")).toBeUndefined();
});

test("kernel nodes register themselves as built-in capabilities exactly once", () => {
  registerCoreNodes(nodes);
  const first = registerKernelTools(nodes, tools);
  const second = registerKernelTools(nodes, tools);
  expect(first.map((row) => row.key).sort()).toEqual(second.map((row) => row.key).sort());
  expect(tools.list().map((row) => row.key)).toEqual([
    "mail.send",
    "vector.index",
    "vector.search",
  ]);
  const index = tools.list().find((row) => row.key === "vector.index");
  expect(index).toMatchObject({ source: "builtin", readOnly: false, tier: "ask" });
  expect(index?.inputSchema).not.toBeNull();
  expect(tools.list().find((row) => row.key === "vector.search")?.tier).toBe("auto");
});

test("the channels expose the registry and match the contract", () => {
  const handlers = createToolHandlers(tools);
  const registered = tools.register(descriptor({ readOnly: true }));
  const listed = handlers["tools.list"](contract["tools.list"].input.parse({}));
  expect(contract["tools.list"].output.parse(listed)).toHaveLength(1);
  expect(
    contract["tools.setTier"].output.parse(
      handlers["tools.setTier"]({ id: registered.id, tier: "off" }),
    ).tier,
  ).toBe("off");
  expect(
    contract["tools.setEnabled"].output.parse(
      handlers["tools.setEnabled"]({ id: registered.id, enabled: false }),
    ).enabled,
  ).toBe(false);
  expect(
    contract["tools.get"].output.parse(handlers["tools.get"]({ id: registered.id })),
  ).toMatchObject({ key: "mail.send", tier: "off", enabled: false });
  expect(
    contract["tools.usage"].output.parse(handlers["tools.usage"]({ id: registered.id })),
  ).toMatchObject({ key: "mail.send", callCount: 0 });
  expect(() =>
    handlers["tools.get"](contract["tools.get"].input.parse({ id: crypto.randomUUID() })),
  ).toThrow();
});

function invoker() {
  return new ToolInvoker({ tools, nodes, runs });
}

test("permission defaults apply to newly registered tools only", () => {
  const settings = new SettingService({ data: db.client });
  tools = new ToolRegistry({ data: db.client, settings, clock: () => now });
  const turnedOff = tools.register(descriptor());
  tools.setTier(turnedOff.id, "off");
  const disabled = tools.register(descriptor({ key: "mail.archive" }));
  tools.setEnabled(disabled.id, false);
  const withdrawn = tools.register(
    descriptor({ key: "mail.forward", source: "mcp", sourceRef: "s" }),
  );
  tools.setTier(withdrawn.id, "off");
  tools.deregisterSource("mcp", "s");

  expect(tools.defaults().builtin).toBe("inferred");
  tools.setDefaults({ builtin: "auto", mcp: "auto" });
  expect(new ToolRegistry({ data: db.client, settings }).defaults()).toMatchObject({
    builtin: "auto",
    mcp: "auto",
    skill: "inferred",
  });

  expect(tools.register(descriptor()).tier).toBe("off");
  expect(tools.register(descriptor({ key: "mail.archive" }))).toMatchObject({
    tier: "ask",
    enabled: false,
  });
  expect(
    tools.register(descriptor({ key: "mail.forward", source: "mcp", sourceRef: "s" })).tier,
  ).toBe("off");
  expect(tools.register(descriptor({ key: "mail.reply" })).tier).toBe("auto");
  expect(tools.register(descriptor({ key: "mail.purge", destructive: true })).tier).toBe("off");
  expect(
    tools.register(descriptor({ key: "web.fetch", source: "browser", readOnly: true })).tier,
  ).toBe("auto");
  tools.setDefaults({ browser: "ask" });
  expect(
    tools.register(descriptor({ key: "web.read", source: "browser", readOnly: true })).tier,
  ).toBe("ask");
  expect(runs.permissions.admitSubject("mail.send", { runId: "r", scopes: [] })).toBe("deny");
});

test("try-it refuses a tool that is off, disabled or unavailable without running it", async () => {
  const registered = tools.register(descriptor({ readOnly: true }));
  tools.setTier(registered.id, "off");
  await expect(invoker().invoke({ id: registered.id, input: {} })).rejects.toMatchObject({
    code: AppErrorCode.PERMISSION_DENIED,
  });
  tools.setTier(registered.id, "auto");
  tools.setEnabled(registered.id, false);
  await expect(
    invoker().invoke({ id: registered.id, input: {}, confirmed: true }),
  ).rejects.toMatchObject({ code: AppErrorCode.PERMISSION_DENIED });
  tools.setEnabled(registered.id, true);
  tools.deregister("mail.send");
  await expect(invoker().invoke({ id: registered.id, input: {} })).rejects.toMatchObject({
    code: AppErrorCode.CONFLICT,
  });
  expect(execute).not.toHaveBeenCalled();
});

test("a destructive tool needs an explicit confirm before try-it runs it", async () => {
  const registered = tools.register(descriptor({ destructive: true }));
  expect(registered.tier).toBe("off");
  tools.setTier(registered.id, "auto");
  await expect(invoker().invoke({ id: registered.id, input: {} })).rejects.toMatchObject({
    code: AppErrorCode.APPROVAL_DENIED,
  });
  expect(execute).not.toHaveBeenCalled();
  expect(runs.list().items).toHaveLength(0);

  const result = await invoker().invoke({ id: registered.id, input: { to: "a" }, confirmed: true });
  expect(result).toMatchObject({ ok: true, output: "executed", error: null });
  expect(execute).toHaveBeenCalledOnce();
  expect(runs.get(result.runId)).toMatchObject({ kind: "job", status: "succeeded" });
  expect(tools.get(registered.id).callCount).toBe(1);
});

test("an ask-tier tool is approved by the try-it confirm, exactly as a run approval", async () => {
  const registered = tools.register(descriptor());
  expect(registered.tier).toBe("ask");
  await expect(invoker().invoke({ id: registered.id, input: {} })).rejects.toMatchObject({
    code: AppErrorCode.APPROVAL_DENIED,
  });
  const result = await invoker().invoke({ id: registered.id, input: {}, confirmed: true });
  expect(result.ok).toBe(true);
  expect(events.some((event) => event.type === "approval")).toBe(true);
  expect(db.client.repositories.permissions.list("mail.send")).toHaveLength(1);
});

test("a failing tool reports the error and the timing instead of throwing", async () => {
  const registered = tools.register(descriptor({ readOnly: true }));
  execute.mockRejectedValueOnce(new Error("upstream exploded"));
  const result = await invoker().invoke({ id: registered.id, input: {} });
  expect(result.ok).toBe(false);
  expect(result.error).toContain("upstream exploded");
  expect(result.durationMs).toBeGreaterThanOrEqual(0);
  expect(runs.get(result.runId).status).toBe("failed");
});

test("the try-it and defaults channels match the contract", async () => {
  const handlers = createToolHandlers(tools, invoker());
  const registered = tools.register(descriptor({ readOnly: true }));
  const result = await handlers["tools.invoke"](
    contract["tools.invoke"].input.parse({ id: registered.id }),
  );
  expect(contract["tools.invoke"].output.parse(result).ok).toBe(true);
  expect(
    contract["tools.setDefaults"].output.parse(
      handlers["tools.setDefaults"](contract["tools.setDefaults"].input.parse({ mcp: "off" })),
    ).mcp,
  ).toBe("off");
  expect(contract["tools.defaults"].output.parse(handlers["tools.defaults"]()).mcp).toBe("off");
  expect(() =>
    createToolHandlers(tools)["tools.invoke"]({ id: registered.id, input: {}, confirmed: false }),
  ).toThrow();
});
