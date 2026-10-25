import { describe, expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AgenticSessionDto,
  contract,
  HostEvent,
  RunSummaryDto,
  type AgenticPlanStepDto,
  type Contract,
  type RunCountsDto,
  type RunDetailDto,
  type StreamId,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import AgenticStore from "../../src/renderer/features/agentic/AgenticStore.ts";
import { draftBudget } from "../../src/renderer/features/agentic/SessionDraft.ts";
import {
  applyEvent,
  budgetLines,
  dropEdit,
  EMPTY_STREAM,
  extendedBudget,
  moveEdit,
  planProgress,
  planRows,
  queuedEdits,
  relativePath,
  renameEdit,
  replay,
  sessionPhase,
  type StreamState,
} from "../../src/renderer/features/agentic/sessionModel.ts";
import {
  cardBody,
  cardResult,
  cardTarget,
  exitCodeLabel,
  outputPreview,
} from "../../src/renderer/features/agentic/toolCardModel.ts";
import type { RunStep } from "../../src/renderer/features/scenarios/runModel.ts";

const SESSION = "0199aa11-8888-7111-8111-111111111111";
const PROVIDER = "0199aa11-3333-7111-8111-111111111111";
const RUN = "0199aa11-4444-7111-8111-111111111111";
const STREAM = "0199aa11-5555-7111-8111-111111111111";
const NEXT_RUN = "0199aa11-4444-7111-8111-222222222222";
const NEXT_STREAM = "0199aa11-5555-7111-8111-222222222222";
const STEP = "0199aa11-6666-7111-8111-111111111111";
const APPROVAL = "0199aa11-7777-7111-8111-111111111111";

function planStep(
  id: string,
  title: string,
  status: AgenticPlanStepDto["status"],
): AgenticPlanStepDto {
  return { id, title, status, stepIds: [], summary: null };
}

function session(patch: Record<string, unknown> = {}): AgenticSessionDto {
  return AgenticSessionDto.parse({
    id: SESSION,
    goal: "Добавить проверку состояния провайдеров",
    status: "running",
    stopReason: null,
    error: null,
    mode: "ask",
    pendingMode: null,
    workspace: "C:\\work\\studio",
    providerId: PROVIDER,
    modelId: "qwen3-coder",
    tools: ["fs.read", "fs.apply"],
    budget: { maxSteps: 25, maxTokens: 200_000, maxMinutes: 15 },
    usage: { steps: 3, tokens: 12_000, elapsedMs: 45_000, toolCalls: 4 },
    plan: [
      planStep("p1", "Прочитать код", "done"),
      planStep("p2", "Изменить сервис", "running"),
      planStep("p3", "Написать тесты", "queued"),
      planStep("p4", "Запустить тесты", "queued"),
    ],
    pendingPlan: null,
    pendingInstructions: [],
    changedFiles: [],
    runId: RUN,
    streamId: STREAM,
    summary: null,
    createdAt: 1,
    updatedAt: 2,
    ...patch,
  });
}

function event(seq: number, body: Record<string, unknown>, streamId: string = STREAM): HostEvent {
  return HostEvent.parse({ streamId, seq, ts: 1, ...body });
}

function toolEvent(
  seq: number,
  status: string,
  extra: Record<string, unknown> = {},
  streamId: string = STREAM,
): HostEvent {
  return event(
    seq,
    {
      type: "step",
      step: {
        id: STEP,
        runId: RUN,
        nodeId: "session.call1",
        type: "fs.apply",
        status,
        input: { conversationId: SESSION, operations: [] },
        output: null,
        error: null,
        startedAt: 100,
        finishedAt: status === "running" ? null : 340,
        attempt: 1,
        ...extra,
      },
    },
    streamId,
  );
}

function sessionEvent(seq: number, value: AgenticSessionDto, streamId: string = STREAM) {
  return event(
    seq,
    { type: "step", step: { domain: "agentic", sessionId: value.id, session: value } },
    streamId,
  );
}

function fold(events: readonly HostEvent[], start: StreamState = EMPTY_STREAM): StreamState {
  return events.reduce((state, next) => applyEvent(state, next).state, start);
}

const DIFF = {
  path: "C:\\work\\studio\\src\\ProviderService.ts",
  kind: "modify",
  added: 2,
  removed: 1,
  diff: "@@ -1,2 +1,3 @@\n-old\n+new\n+more\n context\n",
  truncated: false,
};

describe("session stream", () => {
  test("tokens gather into one text entry until a tool call breaks them", () => {
    const state = fold([
      event(1, { type: "token", delta: "Читаю ", kind: "text" }),
      event(2, { type: "token", delta: "код", kind: "text" }),
      event(3, { type: "token", delta: "размышление", kind: "reasoning" }),
      toolEvent(4, "running"),
      event(5, { type: "token", delta: "Готово", kind: "text" }),
    ]);
    expect(state.entries.map((entry) => entry.kind)).toEqual(["text", "tool", "text"]);
    expect(state.entries[0]).toMatchObject({ kind: "text", text: "Читаю код" });
    expect(state.entries[2]).toMatchObject({ kind: "text", text: "Готово" });
  });

  test("a tool step updates its card in place and late or repeated events are dropped", () => {
    const running = fold([toolEvent(1, "running")]);
    const done = applyEvent(running, toolEvent(2, "succeeded", { output: { text: "ok" } })).state;
    expect(done.entries).toHaveLength(1);
    expect(done.entries[0]).toMatchObject({ kind: "tool", step: { status: "succeeded" } });
    const stale = applyEvent(done, toolEvent(2, "running"));
    expect(stale.state).toBe(done);
    const older = applyEvent(done, toolEvent(1, "running"));
    expect(older.state.entries[0]).toMatchObject({ step: { status: "succeeded" } });
  });

  test("the kernel's own session step and steps of other nodes are not cards", () => {
    const state = fold([
      event(1, {
        type: "step",
        step: {
          id: STEP,
          nodeId: "session",
          type: "agentic.session",
          status: "running",
          startedAt: 1,
        },
      }),
    ]);
    expect(state.entries).toEqual([]);
  });

  test("an agentic step event carries the whole session", () => {
    const next = session({ updatedAt: 9, plan: [planStep("p1", "Прочитать код", "done")] });
    const update = applyEvent(EMPTY_STREAM, sessionEvent(1, next));
    expect(update.session?.updatedAt).toBe(9);
    expect(update.state.entries).toEqual([]);
  });

  test("an approval is shown until its tool step settles or the run ends", () => {
    const asked = fold([
      toolEvent(1, "running"),
      event(2, {
        type: "approval",
        request: {
          id: APPROVAL,
          runId: RUN,
          subject: "fs.apply",
          scope: "run",
          expiresAt: 10,
          action: "изменение файлов",
          changes: [DIFF],
        },
      }),
    ]);
    expect(asked.approval).toMatchObject({ id: APPROVAL, subject: "fs.apply" });
    expect(asked.approval?.changes).toHaveLength(1);
    expect(applyEvent(asked, toolEvent(3, "succeeded")).state.approval).toBeNull();
    const ended = applyEvent(asked, event(3, { type: "end", outcome: { status: "ok" } }));
    expect(ended.ended).toBe(true);
    expect(ended.state.approval).toBeNull();
  });

  test("replay keeps the newest session it saw across runs", () => {
    const first = session({ updatedAt: 5 });
    const second = session({ updatedAt: 3 });
    const result = replay(EMPTY_STREAM, [
      sessionEvent(1, first),
      sessionEvent(1, second, NEXT_STREAM),
      event(2, { type: "token", delta: "текст", kind: "text" }, NEXT_STREAM),
    ]);
    expect(result.session?.updatedAt).toBe(5);
    expect(result.state.entries).toHaveLength(1);
  });
});

describe("plan", () => {
  test("rows keep finished steps and show the user's pending edit for the queue", () => {
    const value = session({
      pendingPlan: [{ id: "p4", title: "Запустить тесты" }, { title: "Обновить документацию" }],
    });
    const rows = planRows(value, true);
    expect(rows.map((row) => [row.title, row.status, row.editable])).toEqual([
      ["Прочитать код", "done", false],
      ["Изменить сервис", "running", false],
      ["Запустить тесты", "queued", true],
      ["Обновить документацию", "queued", true],
    ]);
    expect(planProgress(rows)).toEqual({ done: 1, total: 4, current: 2 });
  });

  test("a pending edit naming a step that already started is left out of the queue", () => {
    const value = session({ pendingPlan: [{ id: "p2", title: "Изменить сервис" }] });
    expect(queuedEdits(value)).toEqual([]);
  });

  test("reorder, rename and delete act on the queued steps only", () => {
    const edits = queuedEdits(session());
    expect(edits).toEqual([
      { id: "p3", title: "Написать тесты" },
      { id: "p4", title: "Запустить тесты" },
    ]);
    expect(moveEdit(edits, 1, 0).map((edit) => edit.id)).toEqual(["p4", "p3"]);
    expect(moveEdit(edits, 0, 5)).toEqual(edits);
    expect(renameEdit(edits, 0, "  Покрыть тестами  ")[0]).toEqual({
      id: "p3",
      title: "Покрыть тестами",
    });
    expect(renameEdit(edits, 0, "   ")).toEqual(edits);
    expect(dropEdit(edits, 0)).toEqual([{ id: "p4", title: "Запустить тесты" }]);
  });

  test("the phase reflects planning, approval, budget and the terminal states", () => {
    const approval = {
      id: APPROVAL,
      runId: RUN,
      subject: "fs.apply",
      action: null,
      changes: [],
      process: null,
    };
    expect(sessionPhase(session({ plan: [] }), null)).toBe("planning");
    expect(sessionPhase(session(), null)).toBe("running");
    expect(sessionPhase(session(), approval)).toBe("approval");
    expect(sessionPhase(session({ status: "paused", stopReason: "tokens" }), null)).toBe("budget");
    expect(sessionPhase(session({ status: "completed", stopReason: "goal" }), null)).toBe(
      "completed",
    );
    expect(sessionPhase(session({ status: "stopped", stopReason: "loop" }), null)).toBe("stopped");
  });
});

describe("budget", () => {
  test("lines report each limit and an extension raises every limit above usage", () => {
    const value = session({
      status: "paused",
      stopReason: "steps",
      usage: { steps: 25, tokens: 150_000, elapsedMs: 61_000, toolCalls: 40 },
    });
    const lines = budgetLines(value.budget, value.usage);
    expect(lines.map((line) => [line.key, line.ratio])).toEqual([
      ["steps", 1],
      ["tokens", 0.75],
      ["time", 61_000 / 900_000],
    ]);
    expect(extendedBudget(value.budget, value.usage)).toEqual({
      maxSteps: 50,
      maxTokens: 400_000,
      maxMinutes: 30,
    });
  });

  test("a budget override fills the empty limits with the defaults", () => {
    expect(draftBudget("", "", "")).toBeUndefined();
    expect(draftBudget("40", "", "")).toEqual({ maxSteps: 40, maxTokens: 200_000, maxMinutes: 15 });
    expect(draftBudget("4x", "", "")).toBeNull();
    expect(draftBudget("", "10", "")).toBeNull();
  });

  test("changed paths are shown relative to the workspace", () => {
    expect(relativePath("C:\\work\\studio\\src\\a.ts", "C:\\work\\studio")).toBe("src\\a.ts");
    expect(relativePath("D:\\other\\a.ts", "C:\\work\\studio")).toBe("D:\\other\\a.ts");
    expect(relativePath("C:\\work\\studio2\\a.ts", "C:\\work\\studio")).toBe(
      "C:\\work\\studio2\\a.ts",
    );
  });
});

function runStep(patch: Partial<RunStep>): RunStep {
  return {
    id: STEP,
    nodeId: "session.call1",
    type: "fs.apply",
    status: "succeeded",
    input: {},
    output: null,
    error: null,
    startedAt: 0,
    finishedAt: 10,
    attempt: 1,
    ...patch,
  };
}

const PROCESS = {
  program: "npm.cmd",
  argv: ["npm", "test"],
  cwd: "C:\\work\\studio",
  packages: [],
};

describe("tool cards", () => {
  test("a file change card carries the diff and no separate result chip", () => {
    const step = runStep({ output: { text: "Изменён 1 файл", changes: [DIFF], paths: [] } });
    const body = cardBody(step);
    expect(body?.kind).toBe("changes");
    expect(cardResult(step)).toBeNull();
  });

  test("a command card shows its output and exit code", () => {
    const ok = runStep({
      type: "shell.run",
      input: { argv: ["npm", "test"], conversationId: SESSION },
      output: {
        text: "",
        isError: false,
        exitCode: 0,
        timedOut: false,
        truncated: false,
        durationMs: 4200,
        stdout: "PASS\n",
        stderr: "",
        process: PROCESS,
      },
    });
    expect(cardTarget(ok)).toBe("npm test");
    expect(cardBody(ok)).toMatchObject({ kind: "process", stdout: "PASS\n", stderr: "" });
    expect(cardResult(ok)).toEqual({ tone: "ok", label: exitCodeLabel(0) });
    const failed = runStep({
      type: "shell.run",
      output: { ...(ok.output as object), exitCode: 1 },
    });
    expect(cardResult(failed)).toEqual({ tone: "err", label: exitCodeLabel(1) });
  });

  test("a failed call shows the host's error", () => {
    const step = runStep({ status: "failed", error: "Действие отклонено" });
    expect(cardBody(step)).toEqual({ kind: "error", text: "Действие отклонено" });
    expect(cardResult(step)?.tone).toBe("err");
  });

  test("long output is collapsed to a preview", () => {
    const text = Array.from({ length: 20 }, (_, index) => `строка ${String(index)}`).join("\n");
    const preview = outputPreview(`${text}\n`, 12);
    expect(preview.hidden).toBe(8);
    expect(preview.shown.split("\n")).toHaveLength(12);
    expect(outputPreview("одна", 12)).toEqual({ shown: "одна", hidden: 0 });
  });
});

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
  byKind: {
    chat: 0,
    scenario: 0,
    agentic: 1,
    job: 0,
    indexing: 0,
    download: 0,
    browser: 0,
    skill: 0,
  },
};

function detail(events: readonly HostEvent[]): RunDetailDto {
  return contract["runs.detail"].output.parse({
    run: {
      id: RUN,
      streamId: STREAM,
      kind: "agentic",
      status: "running",
      graph: { nodes: [{ id: "session", type: "agentic.session" }] },
      input: null,
      concurrency: 1,
      createdAt: 10,
    },
    summary: RunSummaryDto.parse({
      id: RUN,
      streamId: STREAM,
      kind: "agentic",
      title: "Сессия",
      status: "running",
      progress: { done: 0, total: 1 },
      createdAt: 10,
    }),
    steps: [],
    logs: [],
    events,
  });
}

function rig(initial: AgenticSessionDto = session(), history: readonly HostEvent[] = []) {
  let current = initial;
  const bridge = createFakeBridge<Contract>()
    .handle("agentic.list", () => [current])
    .handle("agentic.get", () => current)
    .handle("providers.list", () => [])
    .handle("runs.list", () => ({
      items: [
        RunSummaryDto.parse({
          id: RUN,
          streamId: STREAM,
          kind: "agentic",
          subjectId: SESSION,
          title: "Сессия",
          status: "running",
          progress: { done: 0, total: 1 },
          createdAt: 10,
        }),
      ],
      counts,
    }))
    .handle("runs.detail", () => detail(history))
    .handle("runs.approve", () => undefined)
    .handle("runs.deny", () => undefined)
    .handle("agentic.updatePlan", ({ steps }) => {
      current = session({ pendingPlan: steps, updatedAt: current.updatedAt + 1 });
      return current;
    })
    .handle("agentic.setMode", ({ mode }) => {
      current = session({ pendingMode: mode, updatedAt: current.updatedAt + 1 });
      return current;
    })
    .handle("agentic.instruct", ({ text }) => {
      current = session({ pendingInstructions: [text], updatedAt: current.updatedAt + 1 });
      return current;
    })
    .handle("agentic.extend", ({ budget }) => {
      current = session({ budget, runId: NEXT_RUN, streamId: NEXT_STREAM, updatedAt: 50 });
      return current;
    });
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new AgenticStore(createIpcClient(contract, bridge), events);
  const seqs = new Map<string, number>();
  const emit = (body: Record<string, unknown>, streamId: string = STREAM) => {
    const seq = (seqs.get(streamId) ?? 0) + 1;
    seqs.set(streamId, seq);
    events.dispatch({ streamId: streamId as StreamId, seq, ts: 1, ...body });
  };
  const skip = (streamId: string = STREAM) => {
    seqs.set(streamId, (seqs.get(streamId) ?? 0) + 1);
  };
  const calls = (channel: string) =>
    bridge.calls.filter((call) => call.channel === channel).map((call) => call.payload);
  return {
    store,
    emit,
    skip,
    calls,
    setSession(next: AgenticSessionDto) {
      current = next;
    },
  };
}

describe("agentic store", () => {
  test("opening a running session replays its history and follows the stream", async () => {
    const { store, emit } = rig(session(), [
      event(1, { type: "token", delta: "План готов", kind: "text" }),
    ]);
    await store.mount();
    expect(store.activeId).toBe(SESSION);
    expect(store.stream.entries).toHaveLength(1);
    emit({ type: "token", delta: "повтор", kind: "text" });
    expect(store.stream.entries).toHaveLength(1);
    emit({ type: "token", delta: ". Дальше", kind: "text" });
    expect(store.stream.entries[0]).toMatchObject({ text: "План готов. Дальше" });
  });

  test("a session step event moves the plan forward", async () => {
    const { store, emit } = rig();
    await store.mount();
    expect(store.progress).toEqual({ done: 1, total: 4, current: 2 });
    emit({
      type: "step",
      step: {
        domain: "agentic",
        sessionId: SESSION,
        session: session({
          updatedAt: 7,
          plan: [
            planStep("p1", "Прочитать код", "done"),
            planStep("p2", "Изменить сервис", "done"),
            planStep("p3", "Написать тесты", "running"),
            planStep("p4", "Запустить тесты", "queued"),
          ],
        }),
      },
    });
    expect(store.progress).toEqual({ done: 2, total: 4, current: 3 });
    expect(store.rows.filter((row) => row.editable).map((row) => row.title)).toEqual([
      "Запустить тесты",
    ]);
  });

  test("a missed event resynchronises from the run history", async () => {
    const { store, emit, skip, calls } = rig();
    await store.mount();
    expect(calls("runs.detail")).toHaveLength(1);
    emit({ type: "token", delta: "до пропуска", kind: "text" });
    skip();
    emit({ type: "token", delta: "после пропуска", kind: "text" });
    await vi.waitFor(() => {
      expect(calls("runs.detail")).toHaveLength(2);
    });
    await vi.waitFor(() => {
      expect(store.stream.entries).toHaveLength(1);
    });
  });

  test("approval answers call the run's approve and deny channels", async () => {
    const { store, emit, calls } = rig();
    await store.mount();
    const ask = (id: string) => {
      emit({
        type: "approval",
        request: { id, runId: RUN, subject: "shell.run", scope: "run", expiresAt: 10 },
      });
    };
    ask(APPROVAL);
    expect(store.phase).toBe("approval");
    await store.decide("once");
    expect(calls("runs.approve")).toEqual([{ id: RUN, approvalId: APPROVAL, always: false }]);
    expect(store.stream.approval).toBeNull();
    const second = "0199aa11-7777-7111-8111-222222222222";
    ask(second);
    await store.decide("always");
    expect(calls("runs.approve")[1]).toEqual({ id: RUN, approvalId: second, always: true });
    const third = "0199aa11-7777-7111-8111-333333333333";
    ask(third);
    await store.decide("deny");
    expect(calls("runs.deny")).toEqual([{ id: RUN, approvalId: third }]);
  });

  test("plan edits, mode and instructions go to the host and show as pending", async () => {
    const { store, calls } = rig();
    await store.mount();
    await store.moveStep(1, 0);
    expect(calls("agentic.updatePlan")).toEqual([
      {
        id: SESSION,
        steps: [
          { id: "p4", title: "Запустить тесты" },
          { id: "p3", title: "Написать тесты" },
        ],
      },
    ]);
    expect(store.rows.slice(2).map((row) => row.id)).toEqual(["p4", "p3"]);
    await store.setMode("auto");
    expect(calls("agentic.setMode")).toEqual([{ id: SESSION, mode: "auto" }]);
    expect(store.session?.pendingMode).toBe("auto");
    store.setInstruction("  Не трогай миграции  ");
    await store.instruct();
    expect(calls("agentic.instruct")).toEqual([{ id: SESSION, text: "Не трогай миграции" }]);
    expect(store.instruction).toBe("");
  });

  test("a paused session is extended and its new run is followed", async () => {
    const paused = session({
      status: "paused",
      stopReason: "tokens",
      usage: { steps: 10, tokens: 200_000, elapsedMs: 120_000, toolCalls: 9 },
    });
    const { store, emit, calls } = rig(paused);
    await store.open(paused.id);
    expect(store.phase).toBe("budget");
    await store.extend();
    expect(calls("agentic.extend")).toEqual([
      { id: SESSION, budget: { maxSteps: 50, maxTokens: 400_000, maxMinutes: 30 } },
    ]);
    expect(store.phase).toBe("running");
    emit({ type: "token", delta: "Продолжаю", kind: "text" }, NEXT_STREAM);
    expect(store.stream.entries.at(-1)).toMatchObject({ text: "Продолжаю" });
  });
});
