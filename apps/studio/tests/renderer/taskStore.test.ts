import { afterEach, expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  contract,
  RunId,
  RunSummaryDto,
  StreamId,
  type Contract,
  type RunCountsDto,
  type RunPageDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import TaskStore from "../../src/renderer/features/tasks/TaskStore.ts";
import {
  groupOf,
  groupRuns,
  matchesStatusFilter,
} from "../../src/renderer/features/tasks/runGroups.ts";
import {
  formatElapsed,
  progressPercent,
  runSubline,
} from "../../src/renderer/features/tasks/runPresentation.ts";

const RUN_ID = "0199bb11-1111-7111-8111-000000000001";
const STREAM_ID = "0199bb11-1111-7111-8111-000000000002";
const STEP_ID = "0199bb11-1111-7111-8111-000000000003";
const APPROVAL_ID = "0199bb11-1111-7111-8111-000000000004";

const counts: RunCountsDto = {
  total: 1,
  byStatus: {
    queued: 0,
    running: 1,
    blocked: 0,
    succeeded: 0,
    failed: 0,
    cancelled: 0,
    interrupted: 0,
  },
  byKind: { chat: 0, scenario: 1, agentic: 0, job: 0, indexing: 0, browser: 0 },
};

function summary(patch: Partial<RunSummaryDto> = {}): RunSummaryDto {
  return RunSummaryDto.parse({
    id: RUN_ID,
    streamId: STREAM_ID,
    kind: "scenario",
    title: "Support triage bot",
    status: "running",
    progress: { done: 1, total: 4 },
    createdAt: 1_800_000_000_000,
    startedAt: 1_800_000_000_000,
    ...patch,
  });
}

function setup(page: RunPageDto = { items: [summary()], counts }) {
  let current = page;
  const detail = {
    run: {
      id: RUN_ID,
      streamId: STREAM_ID,
      kind: "scenario",
      status: "running",
      graph: {
        nodes: [
          { id: "classify", type: "llm.generate" },
          { id: "reply", type: "llm.generate", dependencies: ["classify"] },
        ],
      },
      input: { from: "ana@acme.io" },
      concurrency: 4,
      createdAt: 1_800_000_000_000,
    },
    summary: summary(),
    steps: [],
    logs: [],
  };
  const bridge = createFakeBridge<Contract>()
    .handle("runs.list", () => current)
    .handle("runs.detail", () => contract["runs.detail"].output.parse(detail))
    .handle("runs.cancel", () => undefined)
    .handle("runs.retry", () => ({ id: RunId.parse(RUN_ID), streamId: StreamId.parse(STREAM_ID) }))
    .handle("runs.approve", () => undefined)
    .handle("runs.clearFinished", () => ({ removed: 2 }));
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new TaskStore(createIpcClient(contract, bridge), events);
  return {
    store,
    events,
    bridge,
    setPage(next: RunPageDto) {
      current = next;
    },
    emit(event: Record<string, unknown>) {
      events.dispatch({ streamId: STREAM_ID, ts: Date.now(), ...event });
    },
  };
}

afterEach(() => {
  vi.useRealTimers();
});

test("group assignment and status filters are pure functions over the run status", () => {
  expect(groupOf({ status: "running" })).toBe("running");
  expect(groupOf({ status: "queued" })).toBe("queued");
  expect(groupOf({ status: "blocked" })).toBe("attention");
  expect(groupOf({ status: "failed" })).toBe("attention");
  expect(groupOf({ status: "interrupted" })).toBe("attention");
  expect(groupOf({ status: "succeeded" })).toBe("finished");
  expect(groupOf({ status: "cancelled" })).toBe("finished");

  const grouped = groupRuns([
    { status: "running" as const },
    { status: "blocked" as const },
    { status: "failed" as const },
  ]);
  expect(grouped.running).toHaveLength(1);
  expect(grouped.attention).toHaveLength(2);
  expect(grouped.queued).toEqual([]);

  expect(matchesStatusFilter({ status: "blocked" }, "active")).toBe(true);
  expect(matchesStatusFilter({ status: "interrupted" }, "failed")).toBe(true);
  expect(matchesStatusFilter({ status: "succeeded" }, "active")).toBe(false);
});

test("presentation helpers format progress, elapsed time and the row sub-line", () => {
  expect(formatElapsed(161_000)).toBe("02:41");
  expect(formatElapsed(3_755_000)).toBe("1:02:35");
  expect(progressPercent({ done: 4, total: 7 })).toBe(57);
  expect(progressPercent({ done: 0, total: 0 })).toBe(0);
  expect(runSubline(summary({ progress: { done: 3, total: 7 }, activeNodeId: "agent" }))).toBe(
    "шаг 4 из 7 · agent",
  );
  expect(runSubline(summary({ kind: "job", progress: { done: 19, total: 31 } }))).toBe(
    "19 из 31 ед. обработано",
  );
  expect(runSubline(summary({ status: "succeeded", progress: { done: 7, total: 7 } }))).toBe(
    "7 из 7 шагов",
  );
});

test("live events update the row without polling and are applied once, in order", async () => {
  const { store, emit, bridge } = setup();
  await store.mount();
  expect(store.runs).toHaveLength(1);
  const listCalls = bridge.calls.filter((call) => call.channel === "runs.list").length;

  emit({ type: "progress", seq: 1, done: 2, total: 4 });
  expect(store.runs[0]?.progress).toEqual({ done: 2, total: 4 });

  emit({ type: "progress", seq: 0, done: 9, total: 9 });
  expect(store.runs[0]?.progress).toEqual({ done: 2, total: 4 });

  emit({ type: "log", seq: 2, line: { runId: RUN_ID, status: "blocked" } });
  expect(store.runs[0]?.status).toBe("blocked");
  expect(store.groups.attention).toHaveLength(1);

  emit({
    type: "approval",
    seq: 3,
    request: {
      id: "a1",
      runId: RUN_ID,
      subject: "npm run db:migrate",
      scope: "global",
      expiresAt: 1,
    },
  });
  expect(store.runs[0]?.approval?.subject).toBe("npm run db:migrate");

  expect(bridge.calls.filter((call) => call.channel === "runs.list")).toHaveLength(listCalls);
  store.dispose();
});

test("a step event keeps the live step tree current for the selected run", async () => {
  const { store, emit } = setup();
  await store.mount();
  await store.select(store.runs[0]!.id);
  expect(store.graph?.nodes).toHaveLength(2);

  emit({
    type: "step",
    seq: 1,
    step: {
      id: STEP_ID,
      runId: RUN_ID,
      nodeId: "classify",
      type: "llm.generate",
      status: "running",
      input: {},
      output: null,
      error: null,
      startedAt: 1_800_000_000_500,
      finishedAt: null,
      attempt: 1,
    },
  });
  expect(store.steps.map((step) => step.status)).toEqual(["running"]);
  expect(store.runs[0]?.activeNodeId).toBe("classify");

  emit({
    type: "step",
    seq: 2,
    step: {
      id: STEP_ID,
      runId: RUN_ID,
      nodeId: "classify",
      type: "llm.generate",
      status: "succeeded",
      input: {},
      output: { intent: "billing" },
      error: null,
      startedAt: 1_800_000_000_500,
      finishedAt: 1_800_000_001_000,
      attempt: 1,
    },
  });
  expect(store.steps).toHaveLength(1);
  expect(store.steps[0]?.status).toBe("succeeded");
  expect(store.runs[0]?.activeNodeId).toBeUndefined();
  store.dispose();
});

test("a gap in the stream and an unknown stream both fall back to a refetch", async () => {
  vi.useFakeTimers();
  const { store, events, bridge, setPage } = setup();
  await store.mount();
  const before = bridge.calls.filter((call) => call.channel === "runs.list").length;

  events.dispatch({ type: "progress", streamId: STREAM_ID, seq: 1, ts: 1, done: 2, total: 4 });
  events.dispatch({ type: "progress", streamId: STREAM_ID, seq: 9, ts: 1, done: 3, total: 4 });
  setPage({ items: [summary({ progress: { done: 3, total: 4 } })], counts });
  await vi.advanceTimersByTimeAsync(300);
  expect(bridge.calls.filter((call) => call.channel === "runs.list").length).toBe(before + 1);
  expect(store.runs[0]?.progress).toEqual({ done: 3, total: 4 });

  events.dispatch({
    type: "log",
    streamId: "0199bb11-9999-7111-8111-000000000009",
    seq: 0,
    ts: 1,
    line: { status: "queued" },
  });
  await vi.advanceTimersByTimeAsync(300);
  expect(bridge.calls.filter((call) => call.channel === "runs.list").length).toBe(before + 2);
  store.dispose();
});

test("stopping, retrying and answering an approval go through the kernel and refresh", async () => {
  const { store, bridge } = setup({
    items: [
      summary({
        status: "blocked",
        approval: { id: APPROVAL_ID, subject: "mail.send", scope: "global", expiresAt: 2 },
      }),
    ],
    counts,
  });
  await store.mount();
  const id = store.runs[0]!.id;

  await store.decide(id, APPROVAL_ID, true);
  expect(bridge.calls.some((call) => call.channel === "runs.approve")).toBe(true);

  await store.stop(id);
  expect(bridge.calls.some((call) => call.channel === "runs.cancel")).toBe(true);

  await store.retry(id);
  expect(bridge.calls.some((call) => call.channel === "runs.retry")).toBe(true);

  await store.clearFinished();
  expect(bridge.calls.some((call) => call.channel === "runs.clearFinished")).toBe(true);
  store.dispose();
});
