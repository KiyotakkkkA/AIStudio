import { afterEach, beforeEach, expect, test } from "vitest";
import { z } from "zod";
import { RunListFilter, StartRunInput, type RunDto } from "@zvs/shared";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { NodeRegistry } from "../src/host/kernel/NodeRegistry.ts";
import { RunService } from "../src/host/services/RunService.ts";
import { RetentionService } from "../src/host/services/RetentionService.ts";
import { SettingService } from "../src/host/services/SettingService.ts";
import { createEventBus, type EventBus } from "../src/host/platform/events.ts";
import { createId } from "../src/host/platform/ids.ts";

let db: TemporaryDatabase;
let registry: NodeRegistry;
let service: RunService;
let bus: EventBus;
let clock = 1_800_000_000_000;

const filter = (patch: Partial<RunListFilter> = {}) => RunListFilter.parse({ limit: 50, ...patch });

function register(type: string, run: () => Promise<RunDto["input"]>) {
  registry.register({
    type,
    input: z.json(),
    output: z.json(),
    permission: { kind: "none" },
    sideEffectFree: true,
    run,
  });
}

function start(
  kind: RunDto["kind"],
  title: string,
  nodes: unknown[] = [{ id: "a", type: "work" }],
) {
  return service.start(StartRunInput.parse({ kind, title, graph: { nodes } }));
}

beforeEach(() => {
  clock = 1_800_000_000_000;
  db = temporaryDatabase();
  registry = new NodeRegistry();
  bus = createEventBus();
  service = new RunService({
    data: db.client,
    events: bus,
    registry,
    clock: () => clock,
  });
  register("work", () => Promise.resolve({ ok: true }));
  register("boom", () => Promise.reject(new Error("channel_not_found")));
});

afterEach(async () => {
  await service.dispose();
  bus.dispose();
  db.dispose();
});

test("the list is one query over one table: kinds, statuses, search and paging", async () => {
  const first = start("scenario", "Slack notify", [{ id: "notify", type: "boom" }]);
  await service.wait(first.id);
  clock += 1000;
  const second = start("job", "Re-index knowledge base");
  await service.wait(second.id);

  const all = service.list(filter());
  expect(all.items.map((run) => run.title)).toEqual(["Re-index knowledge base", "Slack notify"]);
  expect(all.counts.total).toBe(2);
  expect(all.counts.byKind.scenario).toBe(1);
  expect(all.counts.byStatus.failed).toBe(1);

  expect(service.list(filter({ kinds: ["job"] })).items).toHaveLength(1);
  expect(service.list(filter({ statuses: ["failed"] })).items[0]?.title).toBe("Slack notify");
  expect(service.list(filter({ query: "index" })).items).toHaveLength(1);
  expect(service.list(filter({ query: "notify" })).items[0]?.title).toBe("Slack notify");
  expect(service.list(filter({ from: clock + 1 })).items).toEqual([]);

  const page = service.list(filter({ limit: 1 }));
  expect(page.items).toHaveLength(1);
  expect(page.nextCursor).toBeDefined();
  const next = service.list(filter({ limit: 1, cursor: page.nextCursor }));
  expect(next.items.map((run) => run.title)).toEqual(["Slack notify"]);
  expect(next.nextCursor).toBeUndefined();
});

test("the live view keeps recent failures and drops settled runs", async () => {
  const failed = start("scenario", "Slack notify", [{ id: "notify", type: "boom" }]);
  await service.wait(failed.id);
  const done = start("job", "Re-index");
  await service.wait(done.id);

  expect(service.list(filter({ live: true })).items.map((run) => run.title)).toEqual([
    "Slack notify",
  ]);

  clock += 25 * 60 * 60 * 1000;
  expect(service.list(filter({ live: true })).items).toEqual([]);
});

test("a summary reports progress, the active node and a pending approval", async () => {
  const handle = start("job", "Two steps", [
    { id: "one", type: "work" },
    { id: "two", type: "work", dependencies: ["one"] },
  ]);
  await service.wait(handle.id);
  const summary = service.list(filter()).items[0]!;
  expect(summary.progress).toEqual({ done: 2, total: 2 });
  expect(summary.activeNodeId).toBeUndefined();

  db.client.repositories.permissions.createApproval({
    id: createId(),
    runId: handle.id,
    subject: "mail.send",
    scope: "global",
    expiresAt: clock + 60_000,
  });
  expect(service.list(filter()).items[0]?.approval?.subject).toBe("mail.send");
});

test("retry starts a new run linked to the original and refuses chat and live runs", async () => {
  const failed = start("scenario", "Slack notify", [{ id: "notify", type: "boom" }]);
  await service.wait(failed.id);

  const retried = service.retry(failed.id);
  await service.wait(retried.id);
  const run = service.get(retried.id);
  expect(run.retryOfId).toBe(failed.id);
  expect(run.title).toBe("Slack notify");
  expect(run.graph).toEqual(service.get(failed.id).graph);
  expect(service.list(filter()).items[0]?.retryOfId).toBe(failed.id);

  const chat = start("chat", "Ход чата");
  await service.wait(chat.id);
  expect(() => service.retry(chat.id)).toThrow("страницы чата");

  const live = start("job", "Ещё выполняется");
  db.client.repositories.runs.update(live.id, { status: "running" });
  expect(() => service.retry(live.id)).toThrow("не завершён");
  await service.wait(live.id);
});

test("retention keeps the run summary and drops step payloads past the window", async () => {
  const handle = start("job", "Старое задание");
  await service.wait(handle.id);
  const settings = new SettingService({ data: db.client, clock: () => clock });
  const retention = new RetentionService({
    data: db.client,
    settings,
    clock: () => clock,
  });

  expect(retention.days).toBe(30);
  expect(retention.sweep()).toBe(0);
  expect(service.steps(handle.id)[0]?.output).toEqual({ ok: true });

  clock += 31 * 24 * 60 * 60 * 1000;
  expect(retention.sweep()).toBe(1);
  expect(retention.sweep()).toBe(0);

  const summary = service.list(filter()).items[0]!;
  expect(summary.title).toBe("Старое задание");
  expect(summary.prunedAt).toBe(clock);
  const detail = service.detail(handle.id);
  expect(detail.steps).toHaveLength(1);
  expect(detail.steps[0]?.status).toBe("succeeded");
  expect(detail.steps[0]?.output).toBeUndefined();
  expect(detail.logs).toEqual([]);

  settings.set("runs.retention.days", 7);
  expect(retention.days).toBe(7);
});

test("clearing finished runs spares the ones a conversation still references", async () => {
  const kept = start("chat", "Ход чата");
  await service.wait(kept.id);
  const dropped = start("job", "Задание");
  await service.wait(dropped.id);

  const conversation = db.client.repositories.chat.create({
    id: createId(),
    title: "Диалог",
    providerId: createId(),
    modelId: "model",
    settings: {},
    systemPrompt: "",
    attachedStoreIds: [],
    createdAt: clock,
    updatedAt: clock,
  });
  db.client.repositories.chat.addMessage({
    id: createId(),
    conversationId: conversation.id,
    role: "user",
    content: "Привет",
    citations: [],
    tokensIn: 1,
    tokensOut: 0,
    durationMs: 0,
    runId: kept.id,
    partial: false,
    createdAt: clock,
  });

  expect(service.clearFinished()).toEqual({ removed: 1 });
  expect(service.list(filter()).items.map((run) => run.id)).toEqual([kept.id]);
});
