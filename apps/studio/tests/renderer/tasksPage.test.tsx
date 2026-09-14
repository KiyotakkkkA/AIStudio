import assert from "node:assert/strict";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { contract, RunSummaryDto, type Contract, type RunCountsDto } from "@zvs/shared";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import StoreProvider from "../../src/renderer/app/StoreProvider.tsx";
import TasksPage from "../../src/renderer/pages/TasksPage.tsx";
import RunsPage from "../../src/renderer/pages/RunsPage.tsx";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";

const RUNNING = "0199bb33-1111-7111-8111-000000000001";
const FAILED = "0199bb33-1111-7111-8111-000000000002";
const STREAM = "0199bb33-1111-7111-8111-000000000003";
const APPROVAL = "0199bb33-1111-7111-8111-000000000004";

const counts: RunCountsDto = {
  total: 2,
  byStatus: {
    queued: 0,
    running: 1,
    blocked: 1,
    succeeded: 0,
    failed: 1,
    cancelled: 0,
    interrupted: 0,
  },
  byKind: { chat: 0, scenario: 2, agentic: 0, job: 0, browser: 0 },
};

const blocked = RunSummaryDto.parse({
  id: RUNNING,
  streamId: STREAM,
  kind: "scenario",
  title: "Support triage bot",
  status: "blocked",
  progress: { done: 4, total: 7 },
  approval: { id: APPROVAL, subject: "npm run db:migrate", scope: "global", expiresAt: 1 },
  createdAt: 1_800_000_000_000,
  startedAt: 1_800_000_000_000,
});

const failed = RunSummaryDto.parse({
  id: FAILED,
  streamId: "0199bb33-1111-7111-8111-000000000005",
  kind: "scenario",
  title: "Slack notify",
  status: "failed",
  progress: { done: 2, total: 3 },
  error: "channel_not_found",
  createdAt: 1_800_000_000_000,
  startedAt: 1_800_000_000_000,
  finishedAt: 1_800_000_060_000,
});

function renderTasks() {
  const bridge = createFakeBridge<Contract>()
    .handle("settings.get", ({ key }) => ({ key }))
    .handle("runs.list", () => ({ items: [blocked, failed], counts }))
    .handle("runs.retry", () => ({ id: FAILED, streamId: STREAM }))
    .handle("runs.approve", () => undefined)
    .handle("runs.detail", ({ id }) =>
      contract["runs.detail"].output.parse({
        run: {
          id,
          streamId: STREAM,
          kind: "scenario",
          status: "blocked",
          graph: { nodes: [{ id: "classify", type: "llm.generate" }] },
          input: { from: "ana@acme.io" },
          concurrency: 4,
          createdAt: 1_800_000_000_000,
        },
        summary: blocked,
        steps: [],
        logs: [],
      }),
    );
  render(
    <MemoryRouter>
      <StoreProvider
        environment={{ ipc: createIpcClient(contract, bridge), events: createEventRouter() }}
      >
        <TasksPage />
      </StoreProvider>
    </MemoryRouter>,
  );
  return bridge;
}

test("the Tasks page groups live runs and answers an approval from the row", async () => {
  const bridge = renderTasks();
  await screen.findByText("Support triage bot");
  assert.ok(screen.getByText("Требуют внимания"));
  assert.ok(screen.getByText("channel_not_found"));
  assert.ok(screen.getByText("ожидает решения — npm run db:migrate"));

  fireEvent.click(screen.getByRole("button", { name: "Решить" }));
  await screen.findByText("Требуется решение: npm run db:migrate");
  fireEvent.click(screen.getByRole("button", { name: "Разрешить" }));
  await waitFor(() => {
    assert.ok(bridge.calls.some((call) => call.channel === "runs.approve"));
  });
});

test("a failed run offers a retry that starts a new run", async () => {
  const bridge = renderTasks();
  await screen.findByText("Slack notify");
  fireEvent.click(screen.getByRole("button", { name: "Повторить" }));
  await waitFor(() => {
    assert.deepEqual(bridge.calls.find((call) => call.channel === "runs.retry")?.payload, {
      id: FAILED,
    });
  });
});

test("Runs & Logs lists history and selects the run named in the query string", async () => {
  const bridge = createFakeBridge<Contract>()
    .handle("settings.get", ({ key }) => ({ key }))
    .handle("runs.list", () => ({ items: [failed], counts }))
    .handle("runs.detail", ({ id }) =>
      contract["runs.detail"].output.parse({
        run: {
          id,
          streamId: STREAM,
          kind: "scenario",
          status: "failed",
          graph: { nodes: [{ id: "notify", type: "slack.post" }] },
          input: null,
          concurrency: 4,
          createdAt: 1_800_000_000_000,
        },
        summary: failed,
        steps: [
          {
            id: "0199bb33-1111-7111-8111-000000000006",
            runId: id,
            nodeId: "notify",
            type: "slack.post",
            status: "failed",
            input: { channel: "#ops" },
            error: "channel_not_found",
            startedAt: 1_800_000_000_100,
            finishedAt: 1_800_000_000_900,
            attempt: 1,
          },
        ],
        logs: [{ runId: id, status: "failed" }],
      }),
    );
  render(
    <MemoryRouter initialEntries={[`/runs?run=${FAILED}`]}>
      <StoreProvider
        environment={{ ipc: createIpcClient(contract, bridge), events: createEventRouter() }}
      >
        <RunsPage />
      </StoreProvider>
    </MemoryRouter>,
  );
  await screen.findByText("Шаги (1)");
  fireEvent.click(screen.getByRole("button", { expanded: false }));
  await screen.findByText("Вход");
  assert.ok(screen.getAllByText(/channel_not_found/).length > 0);
});
