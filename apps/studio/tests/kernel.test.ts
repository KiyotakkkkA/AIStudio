import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import {
  HostEvent,
  RunId,
  StartRunInput,
  StreamId,
  contract,
  type GraphNode,
  type RunDto,
} from "@zvs/shared";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { NodeRegistry } from "../src/host/kernel/NodeRegistry.ts";
import type { StepContext } from "../src/host/kernel/types.ts";
import { RunService, type RunServiceOptions } from "../src/host/services/RunService.ts";
import { createEventBus, type EventBus } from "../src/host/platform/events.ts";
import { createId } from "../src/host/platform/ids.ts";
import { createRunHandlers } from "../src/host/ipc/runs.ts";
import { openDatabase } from "../src/host/data/client.ts";

let db: TemporaryDatabase;
let registry: NodeRegistry;
let service: RunService;
let bus: EventBus;
let sent: HostEvent[];
const log = vi.fn();
function setup(options: Partial<RunServiceOptions> = {}) {
  bus = createEventBus({
    senders: () => [
      { isDestroyed: () => false, send: (_channel, event) => sent.push(HostEvent.parse(event)) },
    ],
  });
  service = new RunService({
    data: db.client,
    events: bus,
    registry,
    graceMs: 20,
    logger: { log, close() {} },
    ...options,
  });
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
function register(
  type: string,
  run: (context: StepContext, input: RunDto["input"]) => Promise<RunDto["input"]>,
  sideEffectFree = true,
) {
  registry.register({
    type,
    input: z.json(),
    output: z.json(),
    permission: { kind: "none" },
    sideEffectFree,
    run,
  });
}
function start(nodes: unknown[], concurrency = 4, input: RunDto["input"] = null) {
  return service.start(StartRunInput.parse({ kind: "job", graph: { nodes }, concurrency, input }));
}
beforeEach(() => {
  db = temporaryDatabase();
  registry = new NodeRegistry();
  sent = [];
  log.mockClear();
  setup();
});
afterEach(async () => {
  await service.dispose();
  bus.dispose();
  db.dispose();
});

test("diamond dependencies, deterministic ready order, input bindings and checkpoints", async () => {
  const order: string[] = [];
  for (const type of ["a", "b", "c", "d"])
    register(type, async (_context, input) => {
      order.push(type);
      return { type, input };
    });
  const handle = start(
    [
      {
        id: "d",
        type: "d",
        dependencies: ["b", "c"],
        input: {},
        bindings: { left: { source: "node", nodeId: "b" }, right: { source: "node", nodeId: "c" } },
      },
      { id: "c", type: "c", dependencies: ["a"] },
      { id: "b", type: "b", dependencies: ["a"] },
      { id: "a", type: "a", input: {}, bindings: { original: { source: "run" } } },
    ],
    2,
    { query: "hello" },
  );
  expect(service.get(handle.id).status).toBe("queued");
  await service.wait(handle.id);
  expect(order).toEqual(["a", "b", "c", "d"]);
  expect(service.get(handle.id).status).toBe("succeeded");
  const steps = service.steps(handle.id);
  expect(steps.map((step) => step.status)).toEqual(Array(4).fill("succeeded"));
  expect(steps[0]?.input).toEqual({ original: { query: "hello" } });
  expect(steps[3]?.input).toEqual({
    left: { type: "b", input: null },
    right: { type: "c", input: null },
  });
  for (const step of steps) {
    expect(step.finishedAt).toBeGreaterThanOrEqual(step.startedAt);
    expect(step.attempt).toBe(1);
  }
});

test("concurrency is bounded and a freed slot is refilled while another node is still running", async () => {
  const gates = [deferred<null>(), deferred<null>(), deferred<null>()];
  const entered = [deferred<void>(), deferred<void>(), deferred<void>()];
  let active = 0;
  let peak = 0;
  register("work", async (_context, input) => {
    const index = input as number;
    peak = Math.max(peak, ++active);
    entered[index]!.resolve();
    await gates[index]!.promise;
    active--;
    return null;
  });
  const handle = start(
    [0, 1, 2].map((id) => ({ id: String(id), type: "work", input: id })),
    2,
  );
  await Promise.all([entered[0]!.promise, entered[1]!.promise]);
  expect(service.steps(handle.id)).toHaveLength(2);
  gates[1]!.resolve(null);
  await entered[2]!.promise;
  expect(peak).toBe(2);
  gates[0]!.resolve(null);
  gates[2]!.resolve(null);
  await service.wait(handle.id);
  expect(service.get(handle.id).status).toBe("succeeded");
});

test("cooperative cancellation is prompt and prevents dependent nodes", async () => {
  const entered = deferred<void>();
  register("wait", async ({ signal }) => {
    entered.resolve();
    await new Promise((_, reject) =>
      signal.addEventListener("abort", () => reject(signal.reason), { once: true }),
    );
    return null;
  });
  const handle = start([
    { id: "a", type: "wait" },
    { id: "b", type: "wait", dependencies: ["a"] },
  ]);
  await entered.promise;
  const began = performance.now();
  await service.cancel(handle.id);
  expect(performance.now() - began).toBeLessThan(1000);
  expect(service.get(handle.id).status).toBe("cancelled");
  expect(service.steps(handle.id).map((step) => step.status)).toEqual(["cancelled"]);
  await service.cancel(handle.id);
  expect(sent.filter((event) => event.type === "end")).toHaveLength(1);
});

test("uncooperative steps are abandoned after grace and late output and events cannot change history", async () => {
  const entered = deferred<StepContext>();
  const gate = deferred<null>();
  register("stuck", async (ctx) => {
    entered.resolve(ctx);
    return await gate.promise;
  });
  const handle = start([{ id: "a", type: "stuck" }]);
  const context = await entered.promise;
  const began = performance.now();
  await service.cancel(handle.id);
  expect(performance.now() - began).toBeLessThan(1000);
  expect(service.steps(handle.id)[0]?.status).toBe("abandoned");
  expect(log).toHaveBeenCalledWith(
    "error",
    "kernel",
    "Abandoned node ignored cancellation",
    expect.anything(),
  );
  const count = sent.length;
  gate.resolve(null);
  context.emit(
    HostEvent.parse({
      type: "log",
      streamId: handle.streamId,
      ts: 1,
      seq: 999,
      line: { late: true },
    }),
  );
  await Promise.resolve();
  expect(sent).toHaveLength(count);
  expect(service.get(handle.id).status).toBe("cancelled");
  expect(service.steps(handle.id)[0]?.status).toBe("abandoned");
});

test("cancel before the queued run starts invokes no nodes", async () => {
  const run = vi.fn(async () => null);
  register("work", run);
  const handle = start([{ id: "a", type: "work" }]);
  await service.cancel(handle.id);
  expect(run).not.toHaveBeenCalled();
  expect(service.get(handle.id).status).toBe("cancelled");
});

test("retry failures are separate persisted attempts with bounded linear backoff", async () => {
  let calls = 0;
  register("retry", async () => {
    if (++calls < 3) throw new Error("temporary");
    return "done";
  });
  const handle = start([{ id: "a", type: "retry", retry: { maxAttempts: 3, backoffMs: 15 } }]);
  await service.wait(handle.id);
  const steps = service.steps(handle.id);
  expect(steps.map((step) => [step.attempt, step.status])).toEqual([
    [1, "failed"],
    [2, "failed"],
    [3, "succeeded"],
  ]);
  expect(steps[1]!.startedAt - steps[0]!.finishedAt!).toBeGreaterThanOrEqual(15);
  expect(steps[2]!.startedAt - steps[1]!.finishedAt!).toBeGreaterThanOrEqual(30);
  expect(steps[0]?.error).toBe("temporary");
  expect(steps[2]?.output).toBe("done");
});

test("cancellation interrupts retry backoff without waiting for it", async () => {
  register("retry", async () => {
    throw new Error("temporary");
  });
  const handle = start([{ id: "a", type: "retry", retry: { maxAttempts: 3, backoffMs: 60_000 } }]);
  await vi.waitFor(() => expect(service.steps(handle.id)[0]?.status).toBe("failed"));
  await service.cancel(handle.id);
  expect(service.get(handle.id).status).toBe("cancelled");
  expect(service.steps(handle.id)).toHaveLength(1);
});

test("exhausted failure aborts siblings and never schedules dependents", async () => {
  register("fail", async () => {
    throw new Error("permanent");
  });
  register("stuck", async () => new Promise<null>(() => {}));
  const handle = start([
    { id: "a", type: "fail" },
    { id: "b", type: "stuck" },
    { id: "c", type: "fail", dependencies: ["a"] },
  ]);
  await service.wait(handle.id);
  expect(service.get(handle.id)).toMatchObject({ status: "failed", error: "permanent" });
  expect(service.steps(handle.id).map((step) => step.status)).toEqual(["failed", "abandoned"]);
});

test("input and output schemas are enforced and invalid output is never checkpointed", async () => {
  const run = vi.fn(async () => -1);
  registry.register({
    type: "validated",
    input: z.number().positive(),
    output: z.number().positive(),
    permission: { kind: "none" },
    sideEffectFree: true,
    run,
  });
  const first = start([{ id: "a", type: "validated", input: "bad" }]);
  await service.wait(first.id);
  expect(run).not.toHaveBeenCalled();
  const second = start([{ id: "a", type: "validated", input: 1 }]);
  await service.wait(second.id);
  expect(run).toHaveBeenCalledOnce();
  expect(service.steps(second.id)[0]).toMatchObject({ status: "failed" });
  expect(service.steps(second.id)[0]?.output).toBeUndefined();
});

test.each([
  [
    { id: "a", type: "work" },
    { id: "a", type: "work" },
  ],
  [{ id: "a", type: "work", dependencies: ["missing"] }],
  [
    { id: "a", type: "work", dependencies: ["b"] },
    { id: "b", type: "work", dependencies: ["a"] },
  ],
  [{ id: "a", type: "unknown" }],
  [{ id: "a", type: "work", input: {}, bindings: { x: { source: "node", nodeId: "missing" } } }],
])("invalid graphs create neither rows nor streams: %j", (...nodes) => {
  register("work", async () => null);
  expect(() => start(nodes)).toThrow();
  expect(service.list()).toEqual([]);
  expect(bus.open).toBe(0);
});

test("all state changes use one stream with monotonic sequence and exactly one terminal event", async () => {
  register("work", async (context) => {
    context.emit(
      HostEvent.parse({
        type: "token",
        streamId: StreamId.parse(createId()),
        seq: 900,
        ts: 0,
        delta: "hello",
      }),
    );
    return null;
  });
  const handle = start([{ id: "a", type: "work" }]);
  await service.wait(handle.id);
  expect(new Set(sent.map((event) => event.streamId))).toEqual(new Set([handle.streamId]));
  expect(sent.map((event) => event.seq)).toEqual(sent.map((_, index) => index));
  expect(sent[0]).toMatchObject({ type: "log", line: { status: "queued" } });
  expect(sent.at(-1)).toMatchObject({ type: "end", outcome: { status: "ok" } });
  expect(sent.filter((event) => event.type === "step").map((event) => event.step.status)).toEqual([
    "running",
    "succeeded",
  ]);
  expect(db.client.repositories.runs.events(handle.id)).toEqual(sent);
});

function checkpoint(nodes: unknown[], status: "running" | "blocked" | "queued" = "running") {
  const input = StartRunInput.parse({ kind: "job", graph: { nodes } });
  const id = RunId.parse(createId());
  const streamId = StreamId.parse(createId());
  db.client.repositories.runs.create({
    ...input,
    id,
    streamId,
    status,
    createdAt: 1,
    startedAt: 2,
  });
  db.client.repositories.runs.appendEvent(
    id,
    HostEvent.parse({ type: "log", streamId, seq: 7, ts: 2, line: { status } }),
  );
  return { id, streamId, graph: input.graph };
}
function savedStep(
  id: string,
  node: GraphNode,
  status: "running" | "succeeded" | "failed",
  attempt = 1,
) {
  db.client.repositories.runs.addStep({
    id: createId(),
    runId: id,
    nodeId: node.id,
    type: node.type,
    input: node.input,
    output: status === "succeeded" ? "saved" : null,
    status,
    startedAt: 2,
    finishedAt: status === "running" ? null : 3,
    attempt,
  });
}

test.each(["queued", "running", "blocked"] as const)(
  "boot resumes safe %s runs from a durable checkpoint using the original stream and sequence",
  async (status) => {
    const run = vi.fn(async () => "new");
    register("safe", run);
    const handle = checkpoint(
      [
        { id: "a", type: "safe" },
        { id: "b", type: "safe", dependencies: ["a"] },
      ],
      status,
    );
    savedStep(handle.id, handle.graph.nodes[0]!, "succeeded");
    savedStep(handle.id, handle.graph.nodes[1]!, "running");
    const reopened = openDatabase({ file: db.file });
    await service.dispose();
    bus.dispose();
    setup({ data: reopened });
    try {
      service.recover();
      service.recover();
      await service.wait(handle.id);
      expect(run).toHaveBeenCalledOnce();
      expect(
        service.steps(handle.id).map((step) => [step.nodeId, step.attempt, step.status]),
      ).toEqual([
        ["a", 1, "succeeded"],
        ["b", 1, "abandoned"],
        ["b", 2, "succeeded"],
      ]);
      expect(service.get(handle.id)).toMatchObject({
        status: "succeeded",
        streamId: handle.streamId,
        startedAt: 2,
      });
      expect(sent[0]?.seq).toBe(8);
      expect(sent.map((event) => event.seq)).toEqual(sent.map((_, index) => index + 8));
    } finally {
      await service.dispose();
      reopened.close();
    }
  },
);

test.each(["unsafe", "missing"])(
  "boot marks a graph containing %s interrupted without executing any node",
  async (type) => {
    const run = vi.fn(async () => null);
    register("safe", run);
    register("unsafe", run, false);
    const handle = checkpoint(
      [
        { id: "a", type: "safe" },
        { id: "b", type },
      ],
      "blocked",
    );
    savedStep(handle.id, handle.graph.nodes[0]!, "running");
    service.recover();
    service.recover();
    expect(run).not.toHaveBeenCalled();
    expect(service.get(handle.id).status).toBe("interrupted");
    expect(service.steps(handle.id)[0]?.status).toBe("abandoned");
    expect(sent.filter((event) => event.type === "end")).toHaveLength(1);
  },
);

test("recovery respects exhausted retries and never reexecutes checkpointed success", async () => {
  const run = vi.fn(async () => null);
  register("safe", run);
  const handle = checkpoint([{ id: "a", type: "safe" }]);
  savedStep(handle.id, handle.graph.nodes[0]!, "failed");
  service.recover();
  await service.wait(handle.id);
  expect(run).not.toHaveBeenCalled();
  expect(service.get(handle.id).status).toBe("failed");
  const complete = checkpoint([{ id: "a", type: "safe" }]);
  savedStep(complete.id, complete.graph.nodes[0]!, "succeeded");
  service.recover();
  await service.wait(complete.id);
  expect(run).not.toHaveBeenCalled();
  expect(service.get(complete.id).status).toBe("succeeded");
});

test("shutdown leaves a resumable run and fences abandoned work before database close", async () => {
  const entered = deferred<void>();
  register("safe", async () => {
    entered.resolve();
    return new Promise<null>(() => {});
  });
  const handle = start([{ id: "a", type: "safe" }]);
  await entered.promise;
  await service.dispose();
  expect(service.get(handle.id).status).toBe("running");
  expect(service.steps(handle.id)[0]?.status).toBe("abandoned");
  expect(sent.some((event) => event.type === "end")).toBe(false);
  expect(() => start([])).toThrow();
  bus.dispose();
  registry = new NodeRegistry();
  register("safe", async () => null);
  setup();
  service.recover();
  await service.wait(handle.id);
  expect(service.get(handle.id).status).toBe("succeeded");
});

test("approval hook blocks and resumes the run, with the same event stream", async () => {
  const approval = deferred<"approved">();
  const entered = deferred<void>();
  await service.dispose();
  bus.dispose();
  setup({
    approval: async () => {
      entered.resolve();
      return approval.promise;
    },
  });
  register("gate", async (context) => context.requestApproval({ tool: "test" }));
  const handle = start([{ id: "a", type: "gate" }]);
  await entered.promise;
  expect(service.get(handle.id).status).toBe("blocked");
  approval.resolve("approved");
  await service.wait(handle.id);
  expect(service.get(handle.id).status).toBe("succeeded");
  expect(sent.some((event) => event.type === "approval")).toBe(true);
});

test("cancellation bounds approval waits and late approval cannot revive a run", async () => {
  const approval = deferred<"approved">();
  const entered = deferred<void>();
  await service.dispose();
  bus.dispose();
  setup({
    approval: async () => {
      entered.resolve();
      return approval.promise;
    },
  });
  register("gate", async (context) => context.requestApproval({ tool: "test" }));
  const handle = start([{ id: "a", type: "gate" }]);
  await entered.promise;
  await service.cancel(handle.id);
  approval.resolve("approved");
  await Promise.resolve();
  expect(service.get(handle.id).status).toBe("cancelled");
  expect(sent.at(-1)?.type).toBe("end");
});

test("all five run channels validate outputs, list persisted runs, and handle missing IDs", async () => {
  const handlers = createRunHandlers(service);
  const handle = await handlers["runs.start"](
    StartRunInput.parse({ kind: "chat", graph: { nodes: [] } }),
  );
  expect(contract["runs.start"].output.parse(handle)).toEqual(handle);
  await service.wait(handle.id);
  expect(
    contract["runs.get"].output.parse(await handlers["runs.get"]({ id: handle.id })).status,
  ).toBe("succeeded");
  expect(contract["runs.list"].output.parse(await handlers["runs.list"](undefined))).toHaveLength(
    1,
  );
  expect(
    contract["runs.steps"].output.parse(await handlers["runs.steps"]({ id: handle.id })),
  ).toEqual([]);
  expect(await handlers["runs.cancel"]({ id: handle.id })).toBeUndefined();
  expect(() => service.get(createId())).toThrow("Запуск не найден");
});

test.each(["safe", "unsafe"])(
  "a killed host process recovers %s runs according to the selected policy",
  async (safety) => {
    const child = spawn(
      process.execPath,
      [
        fileURLToPath(new URL("../../../test/helpers/kernelCrash.mjs", import.meta.url)),
        db.file,
        safety,
      ],
      {
        stdio: ["ignore", "ignore", "pipe", "ipc"],
        windowsHide: true,
      },
    );
    let stderr = "";
    child.stderr!.on("data", (chunk: Buffer) => {
      stderr += chunk.toString();
    });
    try {
      const message = await new Promise<{ id: string }>((resolve, reject) => {
        child.once("message", (value) => resolve(value as { id: string }));
        child.once("error", reject);
        child.once("exit", () => reject(new Error(stderr || "Child exited before checkpoint")));
      });
      const exited = once(child, "exit");
      child.kill("SIGKILL");
      await exited;
      const run = vi.fn(async () => "resumed");
      register("crash.work", run, safety === "safe");
      expect(service.steps(message.id).map((step) => step.status)).toEqual([
        "succeeded",
        "running",
      ]);
      const prior = db.client.repositories.runs.events(message.id);
      service.recover();
      await service.wait(message.id);
      expect(service.get(message.id).status).toBe(safety === "safe" ? "succeeded" : "interrupted");
      expect(run).toHaveBeenCalledTimes(safety === "safe" ? 1 : 0);
      expect(service.steps(message.id)[0]?.output).toBe("saved");
      expect(sent[0]?.seq).toBe(prior.at(-1)!.seq + 1);
      expect(sent.at(-1)?.type).toBe("end");
    } finally {
      if (child.exitCode === null && child.signalCode === null) {
        const exited = once(child, "exit");
        child.kill("SIGKILL");
        await exited;
      }
    }
  },
  15_000,
);

test("node mutations cannot rewrite recorded inputs or another node's dependency output", async () => {
  register("mutate", async (_context, input) => {
    (input as Record<string, unknown>).changed = true;
    return input;
  });
  const handle = start([{ id: "a", type: "mutate", input: { original: true } }]);
  await service.wait(handle.id);
  expect(service.steps(handle.id)[0]?.input).toEqual({ original: true });
  expect(service.steps(handle.id)[0]?.output).toEqual({ original: true, changed: true });
  const output = { value: "original" };
  register("source", async () => output);
  register("mutator", async () => {
    output.value = "changed";
    return null;
  });
  register("consumer", async (_context, input) => input);
  const second = start([
    { id: "a", type: "source" },
    { id: "b", type: "mutator", dependencies: ["a"] },
    {
      id: "c",
      type: "consumer",
      dependencies: ["a", "b"],
      input: {},
      bindings: { saved: { source: "node", nodeId: "a" } },
    },
  ]);
  await service.wait(second.id);
  expect(service.steps(second.id)[2]?.output).toEqual({ saved: { value: "original" } });
});

test("renderer delivery failure cannot roll back a checkpoint or fail the run", async () => {
  await service.dispose();
  bus.dispose();
  bus = createEventBus({
    senders: () => [
      {
        isDestroyed: () => false,
        send: () => {
          throw new Error("window closed");
        },
      },
    ],
  });
  service = new RunService({ data: db.client, registry, events: bus });
  register("work", async () => "saved");
  const handle = start([{ id: "a", type: "work" }]);
  await service.wait(handle.id);
  expect(service.get(handle.id).status).toBe("succeeded");
  expect(service.steps(handle.id)[0]?.output).toBe("saved");
  expect(db.client.repositories.runs.events(handle.id).at(-1)?.type).toBe("end");
});

test("nodes needing permission fail closed when the stored ceiling is off", async () => {
  const run = vi.fn(async () => null);
  registry.register({
    type: "tool",
    input: z.json(),
    output: z.json(),
    sideEffectFree: false,
    permission: { tool: "mail.send" },
    run,
  });
  service.permissions.grant("mail.send", "global", "off", "user");
  const handle = start([{ id: "a", type: "tool" }]);
  await service.wait(handle.id);
  expect(run).not.toHaveBeenCalled();
  expect(service.get(handle.id).status).toBe("failed");
});

test("cancelling one run leaves another run and its event stream independent", async () => {
  const entered = [deferred<void>(), deferred<void>()];
  const gates = [deferred<null>(), deferred<null>()];
  const signals: AbortSignal[] = [];
  register("wait", async (context, input) => {
    const index = input as number;
    signals[index] = context.signal;
    entered[index]!.resolve();
    return gates[index]!.promise;
  });
  const first = start([{ id: "a", type: "wait", input: 0 }]);
  const second = start([{ id: "a", type: "wait", input: 1 }]);
  await Promise.all(entered.map((entry) => entry.promise));
  await service.cancel(first.id);
  expect(signals[0]?.aborted).toBe(true);
  expect(signals[1]?.aborted).toBe(false);
  gates[1]!.resolve(null);
  await service.wait(second.id);
  expect(service.get(second.id).status).toBe("succeeded");
  for (const handle of [first, second]) {
    const events = sent.filter((event) => event.streamId === handle.streamId);
    expect(events.map((event) => event.seq)).toEqual(events.map((_, index) => index));
    expect(events.filter((event) => event.type === "end")).toHaveLength(1);
  }
});
