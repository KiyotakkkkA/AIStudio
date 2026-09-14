import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  contract,
  RunSummaryDto,
  type Contract,
  type RunCountsDto,
  type RunListFilter,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import RunHistoryStore from "../../src/renderer/features/tasks/RunHistoryStore.ts";

const STREAM_ID = "0199bb22-1111-7111-8111-000000000002";
const STEP_ID = "0199bb22-1111-7111-8111-000000000003";

const counts: RunCountsDto = {
  total: 2,
  byStatus: {
    queued: 0,
    running: 0,
    blocked: 0,
    succeeded: 1,
    failed: 1,
    cancelled: 0,
    interrupted: 0,
  },
  byKind: { chat: 1, scenario: 1, agentic: 0, job: 0, browser: 0 },
};

function summary(index: number): RunSummaryDto {
  return RunSummaryDto.parse({
    id: `0199bb22-1111-7111-8111-00000000000${index}`,
    streamId: STREAM_ID,
    kind: "scenario",
    title: `Запуск ${index}`,
    status: "succeeded",
    progress: { done: 1, total: 1 },
    createdAt: 1_800_000_000_000 - index,
    startedAt: 1_800_000_000_000 - index,
    finishedAt: 1_800_000_001_000 - index,
  });
}

function setup() {
  const filters: RunListFilter[] = [];
  const bridge = createFakeBridge<Contract>()
    .handle("runs.list", (filter) => {
      filters.push(filter);
      return {
        items: [summary(1)],
        counts,
        ...(filter.cursor === undefined ? { nextCursor: "1800000000000:a" } : {}),
      };
    })
    .handle("runs.detail", ({ id }) =>
      contract["runs.detail"].output.parse({
        run: {
          id,
          streamId: STREAM_ID,
          kind: "scenario",
          status: "succeeded",
          graph: { nodes: [{ id: "a", type: "work" }] },
          input: null,
          concurrency: 4,
          createdAt: 1_800_000_000_000,
        },
        summary: { ...summary(1), id },
        steps: [
          {
            id: STEP_ID,
            runId: id,
            nodeId: "a",
            type: "work",
            status: "succeeded",
            input: { q: 1 },
            output: { ok: true },
            startedAt: 1_800_000_000_100,
            finishedAt: 1_800_000_000_400,
            attempt: 1,
          },
        ],
        logs: [{ runId: id, status: "succeeded" }],
      }),
    );
  return { store: new RunHistoryStore(createIpcClient(contract, bridge)), bridge, filters };
}

test("filters, search and paging travel to the host as one run query", async () => {
  vi.useFakeTimers();
  try {
    const { store, filters } = setup();
    await store.mount();
    expect(filters[0]).toMatchObject({ live: false, limit: 40 });
    expect(store.hasMore).toBe(true);
    expect(store.filtered).toBe(false);

    store.toggleStatus("failed");
    store.toggleKind("chat");
    store.setRange("week");
    await vi.advanceTimersByTimeAsync(0);
    const latest = filters.at(-1)!;
    expect(latest.statuses).toEqual(["failed"]);
    expect(latest.kinds).toEqual(["chat"]);
    expect(latest.from).toBeGreaterThan(0);
    expect(store.filtered).toBe(true);

    store.setQuery("slack");
    await vi.advanceTimersByTimeAsync(300);
    expect(filters.at(-1)?.query).toBe("slack");

    await store.loadMore();
    expect(filters.at(-1)?.cursor).toBe("1800000000000:a");
    expect(store.runs).toHaveLength(2);
    expect(store.hasMore).toBe(false);

    store.reset();
    await vi.advanceTimersByTimeAsync(0);
    expect(store.filtered).toBe(false);
    store.dispose();
  } finally {
    vi.useRealTimers();
  }
});

test("a selected run carries every step with its input, output and log lines", async () => {
  const { store } = setup();
  await store.mount();
  await store.select(store.runs[0]!.id);
  expect(store.detail?.steps[0]?.input).toEqual({ q: 1 });
  expect(store.detail?.steps[0]?.output).toEqual({ ok: true });
  expect(store.detail?.logs).toHaveLength(1);

  expect(store.expandedStepId).toBeNull();
  store.toggleStep(STEP_ID);
  expect(store.expandedStepId).toBe(STEP_ID);
  store.toggleStep(STEP_ID);
  expect(store.expandedStepId).toBeNull();
  store.dispose();
});

test("a failing host leaves a message and keeps the list usable", async () => {
  const bridge = createFakeBridge<Contract>();
  const store = new RunHistoryStore(createIpcClient(contract, bridge));
  await store.mount();
  expect(store.error).not.toBeNull();
  expect(store.runs).toEqual([]);
  store.dispose();
});
