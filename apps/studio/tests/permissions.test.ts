import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { z } from "zod";
import { AppErrorCode, HostEvent, StartRunInput, RunId, type HostEventDraft } from "@zvs/shared";
import type { StepContext } from "../src/host/kernel/types.ts";
import type { TextGenerationDriver } from "../src/host/drivers/ai/ports.ts";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { NodeRegistry } from "../src/host/kernel/NodeRegistry.ts";
import { registerCoreNodes } from "../src/host/kernel/coreNodes.ts";
import { RunService } from "../src/host/services/RunService.ts";
import { createEventBus, type EventBus } from "../src/host/platform/events.ts";
import { createRunHandlers } from "../src/host/ipc/runs.ts";

let db: TemporaryDatabase;
let registry: NodeRegistry;
let service: RunService;
let bus: EventBus;
let events: HostEvent[];
const execute = vi.fn(async () => "executed");
beforeEach(() => {
  db = temporaryDatabase();
  events = [];
  execute.mockClear();
  registry = new NodeRegistry();
  registry.register({
    type: "tool",
    input: z.json(),
    output: z.string(),
    sideEffect: true,
    permission: { tool: "mail.send" },
    run: execute,
  });
  bus = createEventBus({
    senders: () => [
      { isDestroyed: () => false, send: (_channel, event) => events.push(HostEvent.parse(event)) },
    ],
  });
  service = new RunService({ data: db.client, registry, events: bus, approvalTimeoutMs: 1000 });
});
afterEach(async () => {
  await service.dispose();
  bus.dispose();
  db.dispose();
  vi.useRealTimers();
});
function start(scopes: string[] = []) {
  return service.start(
    StartRunInput.parse({
      kind: "job",
      graph: {
        permissionScopes: scopes,
        nodes: [{ id: "a", type: "tool", retry: { maxAttempts: 3, backoffMs: 0 } }],
      },
    }),
  );
}
async function approval() {
  await vi.waitFor(() => expect(events.some((event) => event.type === "approval")).toBe(true));
  const event = events.find((event) => event.type === "approval");
  if (event?.type !== "approval") throw new Error("Missing approval");
  return String(event.request.id);
}
const tiers = ["auto", "ask", "off"] as const;
test.each(
  tiers.flatMap((global, i) =>
    tiers.map((scope, j) => ({
      global,
      scope,
      expected: ["allow", "ask", "deny"][Math.max(i, j)],
    })),
  ),
)("$global ceiling with $scope narrowing yields $expected", ({ global, scope, expected }) => {
  service.permissions.grant("mail.send", "global", global, "user");
  service.permissions.grant("mail.send", "skill:test", scope, "user");
  expect(service.permissions.admit("tool", { runId: "test", scopes: ["skill:test"] })).toBe(
    expected,
  );
});
test("combines all applicable scopes and ignores unrelated grants", () => {
  service.permissions.grant("mail.send", "global", "auto", "user");
  service.permissions.grant("mail.send", "skill:a", "ask", "user");
  service.permissions.grant("mail.send", "site:example.com", "off", "user");
  expect(
    service.permissions.admit("tool", { runId: "r", scopes: ["skill:a", "site:example.com"] }),
  ).toBe("deny");
  expect(service.permissions.admit("tool", { runId: "r", scopes: [] })).toBe("allow");
});

test("a node requirement can narrow a global auto grant at step admission", async () => {
  registry.register({
    type: "careful",
    input: z.json(),
    output: z.string(),
    sideEffect: true,
    permission: { tool: "mail.send", tier: "ask" },
    run: execute,
  });
  service.permissions.grant("mail.send", "global", "auto", "user");
  expect(service.permissions.admit("careful", { runId: "r", scopes: [] })).toBe("ask");
  const handle = service.start(
    StartRunInput.parse({ kind: "job", graph: { nodes: [{ id: "a", type: "careful" }] } }),
  );
  const id = await approval();
  expect(execute).not.toHaveBeenCalled();
  service.deny(handle.id, id);
  await service.wait(handle.id);
  expect(service.get(handle.id).outcome?.code).toBe(AppErrorCode.APPROVAL_DENIED);
});
test("unknown nodes reject the entire graph before any run or step is created", () => {
  expect(() =>
    service.start(
      StartRunInput.parse({
        kind: "job",
        graph: {
          nodes: [
            { id: "a", type: "tool" },
            { id: "b", type: "missing", dependencies: ["a"] },
          ],
        },
      }),
    ),
  ).toThrow();
  expect(service.list().items).toEqual([]);
  expect(execute).not.toHaveBeenCalled();
});
test("approval persists before emission, blocks execution, then resumes via IPC", async () => {
  const handle = start();
  const id = await approval();
  expect(service.get(handle.id).status).toBe("blocked");
  expect(db.client.repositories.permissions.approval(id)?.decision).toBeNull();
  expect(execute).not.toHaveBeenCalled();
  await createRunHandlers(service)["runs.approve"]({
    id: handle.id,
    approvalId: id,
    always: false,
  });
  await service.wait(handle.id);
  expect(service.get(handle.id).status).toBe("succeeded");
  expect(execute).toHaveBeenCalledTimes(1);
  expect(() => service.approve(handle.id, id)).toThrow();
});
test("denial fails with APPROVAL_DENIED without retries", async () => {
  const handle = start();
  const id = await approval();
  await createRunHandlers(service)["runs.deny"]({ id: handle.id, approvalId: id });
  await service.wait(handle.id);
  expect(service.get(handle.id).outcome?.code).toBe(AppErrorCode.APPROVAL_DENIED);
  expect(service.steps(handle.id)).toHaveLength(1);
  expect(execute).not.toHaveBeenCalled();
});
test("timeout denies and never leaves a blocked run", async () => {
  const handle = start();
  const id = await approval();
  await service.wait(handle.id);
  expect(service.get(handle.id).outcome?.code).toBe(AppErrorCode.APPROVAL_DENIED);
  expect(db.client.repositories.permissions.approval(id)?.decision).toBe("denied");
  expect(execute).not.toHaveBeenCalled();
});
test("always allow grants are persisted, logged, honoured and revocable", async () => {
  const handle = start();
  const id = await approval();
  service.approve(handle.id, id, true);
  await service.wait(handle.id);
  const grant = db.client.repositories.permissions.list("mail.send")[0]!;
  expect(grant.scope).toBe("global");
  const next = start();
  await service.wait(next.id);
  expect(db.client.repositories.permissions.uses(grant.id).map((use) => use.runId)).toContain(
    next.id,
  );
  service.permissions.revoke(grant.id);
  expect(db.client.repositories.permissions.uses(grant.id)[0]).toMatchObject({
    subject: "mail.send",
    scope: "global",
    tier: "auto",
    grantedBy: "user",
  });
  expect(service.permissions.admit("tool", { runId: "r", scopes: [] })).toBe("ask");
});
test("scoped always allow cannot widen an ask ceiling", async () => {
  service.permissions.grant("mail.send", "global", "ask", "user");
  const handle = start(["skill:a"]);
  service.approve(handle.id, await approval(), true);
  await service.wait(handle.id);
  expect(service.permissions.admit("tool", { runId: "r", scopes: ["skill:a"] })).toBe("ask");
});
test("revocation while pending cannot be overridden by always allow", async () => {
  const handle = start();
  const id = await approval();
  service.permissions.grant("mail.send", "global", "off", "user");
  expect(() => service.approve(handle.id, id, true)).toThrow();
  service.deny(handle.id, id);
  await service.wait(handle.id);
  expect(execute).not.toHaveBeenCalled();
  expect(db.client.repositories.permissions.list("mail.send")[0]?.tier).toBe("off");
});
test("cancellation resolves pending approval and rejects late decisions", async () => {
  const handle = start();
  const id = await approval();
  await service.cancel(handle.id);
  expect(db.client.repositories.permissions.approval(id)?.decision).toBe("denied");
  expect(() => service.approve(handle.id, id)).toThrow();
  expect(execute).not.toHaveBeenCalled();
});
test("core branch and map compose through ordinary graph bindings", async () => {
  registerCoreNodes(registry);
  const handle = service.start(
    StartRunInput.parse({
      kind: "job",
      graph: {
        nodes: [
          {
            id: "branch",
            type: "flow.branch",
            input: { condition: true, then: [{ name: "A" }, { name: "B" }], else: [] },
          },
          {
            id: "map",
            type: "flow.map",
            dependencies: ["branch"],
            input: { path: ["name"] },
            bindings: { items: { source: "node", nodeId: "branch" } },
          },
        ],
      },
    }),
  );
  await service.wait(handle.id);
  expect(service.steps(handle.id).at(-1)?.output).toEqual(["A", "B"]);
});

test("core generation streams text and reasoning through the provider port", async () => {
  registerCoreNodes(registry);
  const drafts: HostEventDraft[] = [];
  const stream = vi.fn<TextGenerationDriver["stream"]>(async function* () {
    yield { text: "Thinking", kind: "reasoning" };
    yield { text: "Hello", kind: "text" };
    yield { text: " world", kind: "text" };
  });
  const text = vi.fn(async () => ({
    stream,
    capabilities: vi.fn(),
    generate: vi.fn(),
    listModels: vi.fn(),
  }));
  const context: StepContext = {
    runId: RunId.parse(crypto.randomUUID()),
    signal: new AbortController().signal,
    emit: (event) => drafts.push(event),
    requestApproval: async () => "denied",
    services: { providers: { text, ephemeralDriver: vi.fn() } },
  };
  const result = await registry.resolve("llm.generate").execute(context, {
    providerId: "provider",
    model: "model",
    messages: [{ role: "user", content: "Hi" }],
  });
  expect(text).toHaveBeenCalledWith("provider");
  expect(stream).toHaveBeenCalledWith(
    { model: "model", messages: [{ role: "user", content: "Hi" }] },
    context.signal,
  );
  expect(result).toEqual({ text: "Hello world", reasoning: "Thinking" });
  expect(drafts).toEqual([
    { type: "token", delta: "Thinking", kind: "reasoning" },
    { type: "token", delta: "Hello", kind: "text" },
    { type: "token", delta: " world", kind: "text" },
  ]);
  expect(registry.resolve("llm.generate").sideEffectFree).toBe(false);
});

test("core vector search delegates validated options and observes cancellation", async () => {
  registerCoreNodes(registry);
  const search = vi.fn(async () => []);
  const controller = new AbortController();
  const context: StepContext = {
    runId: RunId.parse(crypto.randomUUID()),
    signal: controller.signal,
    emit: vi.fn(),
    requestApproval: async () => "denied",
    services: { vectorStores: { search } },
  };
  const storeId = crypto.randomUUID();
  expect(
    await registry
      .resolve("vector.search")
      .execute(context, { storeId, query: "query", k: 3, minScore: 0.5 }),
  ).toEqual([]);
  expect(search).toHaveBeenCalledWith(storeId, "query", { k: 3, minScore: 0.5 }, controller.signal);
  controller.abort();
  await expect(
    registry.resolve("vector.search").execute(context, { storeId, query: "query" }),
  ).rejects.toThrow();
  expect(search).toHaveBeenCalledTimes(1);
});

test("restart fails interrupted approval without repeating the protected operation", async () => {
  const handle = start();
  await approval();
  await service.dispose();
  bus.dispose();
  bus = createEventBus({ senders: () => [] });
  service = new RunService({ data: db.client, registry, events: bus });
  service.recover();
  await service.wait(handle.id);
  expect(service.get(handle.id).outcome?.code).toBe(AppErrorCode.APPROVAL_DENIED);
  expect(execute).not.toHaveBeenCalled();
});
