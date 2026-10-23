import { describe, expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  contract,
  type Contract,
  type Json,
  type RunId,
  type ScenarioGraphDto,
  type ScenarioId,
  type ScenarioNodeDto,
  type ScenarioNodeTypeDto,
  type StreamId,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createFakeClock } from "../../../../test/helpers/fakeClock.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import { applyAction, edgeLinkId } from "../../src/renderer/features/scenarios/graphModel.ts";
import { jsonTokens } from "../../src/renderer/features/scenarios/jsonTokens.ts";
import {
  formFields,
  NODE_FORMS,
  schemaFields,
  withField,
} from "../../src/renderer/features/scenarios/nodeForms.ts";
import {
  edgeStates,
  failedStep,
  finishView,
  itemCount,
  nodeStates,
  startView,
  stepRows,
  upstreamOutput,
  withStep,
  type RunStep,
  type RunView,
} from "../../src/renderer/features/scenarios/runModel.ts";
import ScenarioRunState, {
  STRIP_HEIGHT_DEFAULT,
  STRIP_HEIGHT_MIN,
} from "../../src/renderer/features/scenarios/ScenarioRunState.ts";
import { itemsLabel } from "../../src/renderer/features/scenarios/runCopy.ts";

const SCENARIO = "0199aa11-1111-7111-8111-111111111111" as ScenarioId;
const RUN = "0199aa11-3333-7333-8333-333333333333" as RunId;
const RETRY = "0199aa11-4444-7444-8444-444444444444" as RunId;
const STREAM = "0199aa11-5555-7555-8555-555555555555" as StreamId;
const RETRY_STREAM = "0199aa11-6666-7666-8666-666666666666" as StreamId;

function node(id: string, type: string, extra: Partial<ScenarioNodeDto> = {}): ScenarioNodeDto {
  return { id, type, position: { x: 0, y: 0 }, params: {}, ...extra };
}

const GRAPH: ScenarioGraphDto = {
  nodes: [
    node("trigger", "trigger.chat"),
    node("search", "vector.search"),
    node("reply", "output.chatReply"),
  ],
  edges: [
    { from: "trigger", to: "search", fromPort: "main", toPort: "main" },
    { from: "search", to: "reply", fromPort: "main", toPort: "main" },
  ],
  attachments: [],
};

let sequence = 0;

function step(nodeId: string, status: RunStep["status"], extra: Partial<RunStep> = {}): RunStep {
  sequence += 1;
  return {
    id: `step-${String(sequence)}`,
    nodeId,
    type: "test",
    status,
    input: {},
    output: null,
    error: null,
    startedAt: 100 + sequence,
    finishedAt: status === "running" ? null : 120 + sequence,
    attempt: 1,
    ...extra,
  };
}

function view(steps: readonly RunStep[], phase: RunView["phase"] = "running"): RunView {
  let current = startView(RUN, STREAM, 100);
  for (const entry of steps) current = withStep(current, entry);
  return phase === "running" ? current : finishView(current, phase, 500, null);
}

describe("run model", () => {
  test("node states follow the latest attempt and ignore tool calls nested under a step", () => {
    const states = nodeStates(
      view([
        step("trigger", "succeeded"),
        step("search", "failed", { error: "сбой" }),
        step("search", "running", { attempt: 2 }),
        step("search.call1", "failed"),
      ]),
    );
    expect(states.get("trigger")).toEqual({ status: "succeeded", error: null });
    expect(states.get("search")).toEqual({ status: "running", error: null });
    expect(states.has("search.call1")).toBe(false);
  });

  test("a failed step is marked with its error and is the one the log opens on", () => {
    const run = view(
      [step("trigger", "succeeded"), step("search", "failed", { error: "Нет ответа" })],
      "failed",
    );
    expect(nodeStates(run).get("search")).toEqual({ status: "failed", error: "Нет ответа" });
    expect(failedStep(run)?.nodeId).toBe("search");
  });

  test("an edge turns traversed once its target starts and counts the items it carried", () => {
    const run = view([
      step("trigger", "succeeded", { output: { text: "привет" } }),
      step("search", "succeeded", { output: { hits: [1, 2] } }),
    ]);
    const states = edgeStates(GRAPH, run);
    const [first, second] = GRAPH.edges;
    expect(states.get(edgeLinkId(first!))).toEqual({ traversed: true, items: 1 });
    expect(states.get(edgeLinkId(second!))).toBeUndefined();
    expect(itemCount([1, 2, 3])).toBe(3);
    expect(itemCount({ hits: [] })).toBe(0);
    expect(itemCount(null)).toBe(0);
    expect(itemsLabel(2)).toBe("2 элемента");
  });

  test("the step tree nests tool calls under their step with timings", () => {
    const rows = stepRows(
      view(
        [
          step("trigger", "succeeded"),
          step("agent", "succeeded"),
          step("agent.call1", "succeeded"),
        ],
        "succeeded",
      ),
    );
    expect(rows.map((row) => [row.step.nodeId, row.depth])).toEqual([
      ["trigger", 0],
      ["agent", 0],
      ["agent.call1", 1],
    ]);
    expect(rows[0]?.duration).toBe(20);
  });

  test("the upstream output of a step is the predecessor's output", () => {
    const run = view([
      step("trigger", "succeeded", { output: { text: "вход" } }),
      step("search", "succeeded", { output: [1] }),
    ]);
    expect(upstreamOutput(GRAPH, run, "search")).toEqual({ text: "вход" });
    expect(upstreamOutput(GRAPH, run, "reply")).toEqual([1]);
    expect(upstreamOutput(GRAPH, run, "trigger")).toBeUndefined();
  });
});

describe("inspector fields", () => {
  const spec: ScenarioNodeTypeDto = {
    type: "vendor.lookup",
    role: "action",
    trigger: null,
    fills: [],
    slots: [],
    requiredParams: ["query"],
    toolKey: null,
    paramsSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "Что искать" },
        limit: { type: ["integer", "null"] },
        exact: { type: "boolean" },
        mode: { type: "string", enum: ["fast", "deep"] },
        filters: { type: "object" },
      },
      required: ["query"],
    },
    idempotent: true,
  };

  test("a new node type gets an editor from its params schema", () => {
    const fields = formFields(spec, spec.type);
    expect(fields.map((field) => [field.key, field.kind, field.required])).toEqual([
      ["query", "text", true],
      ["limit", "number", false],
      ["exact", "boolean", false],
      ["mode", "select", false],
      ["filters", "json", false],
    ]);
    expect(fields[0]?.hint).toBe("Что искать");
    expect(schemaFields(undefined)).toEqual([]);
  });

  test("a json field stores parsed json and numbers are parsed", () => {
    const [, limit, , , filters] = formFields(spec, spec.type);
    expect(withField({}, filters!, '{"lang":"ru"}')).toEqual({ filters: { lang: "ru" } });
    expect(withField({}, limit!, "5")).toEqual({ limit: 5 });
  });

  test("declared form fields keep their labels ahead of schema properties", () => {
    const fields = formFields(
      {
        ...spec,
        type: "output.notify",
        paramsSchema: {
          type: "object",
          properties: { title: { type: "string" }, extra: { type: "string" } },
        },
      },
      "output.notify",
    );
    expect(fields.map((field) => field.key)).toEqual(["title", "template", "extra", "query"]);
    expect(fields[0]?.label).toBe(NODE_FORMS["output.notify"]?.fields[0]?.label);
  });
});

describe("json highlighting", () => {
  test("keys and string values are told apart", () => {
    const tokens = jsonTokens({ query: "токен", k: 5, ok: true });
    const kinds = tokens.filter((token) => token.kind !== "space" && token.kind !== "punctuation");
    expect(kinds.map((token) => [token.kind, token.text])).toEqual([
      ["key", '"query"'],
      ["string", '"токен"'],
      ["key", '"k"'],
      ["number", "5"],
      ["key", '"ok"'],
      ["literal", "true"],
    ]);
  });
});

describe("node patches", () => {
  test("settings, notes and pinned input are set and cleared", () => {
    const graph: ScenarioGraphDto = {
      nodes: [node("a", "trigger.chat")],
      edges: [],
      attachments: [],
    };
    const set = applyAction(
      graph,
      {
        type: "updateNode",
        nodeId: "a",
        patch: { settings: { maxAttempts: 2 }, notes: "проверить", pinnedInput: { text: "x" } },
      },
      new Map(),
    );
    if (!set.ok) throw new Error("refused");
    expect(set.graph.nodes[0]).toMatchObject({
      settings: { maxAttempts: 2 },
      notes: "проверить",
      pinnedInput: { text: "x" },
    });
    const cleared = applyAction(
      set.graph,
      { type: "updateNode", nodeId: "a", patch: { settings: {}, notes: " ", pinnedInput: null } },
      new Map(),
    );
    if (!cleared.ok) throw new Error("refused");
    expect(cleared.graph.nodes[0]).toEqual(node("a", "trigger.chat"));
  });
});

function stepEvent(id: string, nodeId: string, status: string, extra: Record<string, Json> = {}) {
  return {
    type: "step" as const,
    step: {
      id,
      runId: RUN,
      nodeId,
      type: "test",
      status,
      input: {},
      output: null,
      error: null,
      startedAt: 10,
      finishedAt: status === "running" ? null : 20,
      attempt: 1,
      ...extra,
    },
  };
}

function rig() {
  const bridge = createFakeBridge<Contract>();
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const clock = createFakeClock();
  const calls: { channel: string; input: unknown }[] = [];
  bridge
    .handle("scenarios.run", (input) => {
      calls.push({ channel: "scenarios.run", input });
      return { id: RUN, streamId: STREAM };
    })
    .handle("scenarios.runNode", (input) => {
      calls.push({ channel: "scenarios.runNode", input });
      return { id: RUN, streamId: STREAM };
    })
    .handle("scenarios.retryStep", (input) => {
      calls.push({ channel: "scenarios.retryStep", input });
      return { id: RETRY, streamId: RETRY_STREAM };
    });
  const state = new ScenarioRunState(createIpcClient(contract, bridge), SCENARIO, events, clock);
  let seq = 0;
  const emit = (streamId: StreamId, event: Record<string, unknown>) => {
    seq += 1;
    events.dispatch({ streamId, seq, ts: 1, ...event });
  };
  return { state, emit, calls, bridge };
}

describe("scenario run state", () => {
  test("a test run follows its stream and marks nodes as steps report", async () => {
    const { state, emit, calls } = rig();
    expect(await state.start(GRAPH)).toBe(true);
    expect(calls[0]).toEqual({ channel: "scenarios.run", input: { id: SCENARIO, graph: GRAPH } });
    emit(STREAM, stepEvent("s1", "trigger", "running"));
    expect(state.nodes.get("trigger")?.status).toBe("running");
    emit(STREAM, stepEvent("s1", "trigger", "succeeded", { output: { text: "привет" } }));
    emit(STREAM, stepEvent("s2", "search", "running"));
    emit(STREAM, { type: "log", line: { message: "поиск", nodeId: "search", level: "info" } });
    expect(state.nodes.get("trigger")?.status).toBe("succeeded");
    expect(state.edges(GRAPH).get(edgeLinkId(GRAPH.edges[0]!))?.traversed).toBe(true);
    emit(STREAM, stepEvent("s2", "search", "succeeded", { output: [1, 2] }));
    emit(STREAM, { type: "end", outcome: { status: "ok" } });
    expect(state.view?.phase).toBe("succeeded");
    expect(state.running).toBe(false);
    expect(state.view?.logs.map((log) => log.message)).toEqual(["поиск"]);
  });

  test("a failed run opens the log strip on the failing step", async () => {
    const { state, emit } = rig();
    state.toggleStrip();
    await state.start(GRAPH);
    emit(STREAM, stepEvent("s1", "trigger", "succeeded"));
    emit(STREAM, stepEvent("s2", "search", "failed", { error: "Хранилище недоступно" }));
    emit(STREAM, {
      type: "end",
      outcome: { status: "failed", code: "RUN_FAILED", message: "Хранилище недоступно" },
    });
    expect(state.view?.phase).toBe("failed");
    expect(state.selected?.nodeId).toBe("search");
    expect(state.tab).toBe("logs");
    expect(state.stripOpen).toBe(true);
    expect(state.nodes.get("search")).toEqual({ status: "failed", error: "Хранилище недоступно" });
  });

  test("running a single step sends its input", async () => {
    const { state, calls } = rig();
    await state.runNode(GRAPH, "search", { text: "закреплено" });
    expect(calls[0]).toEqual({
      channel: "scenarios.runNode",
      input: { id: SCENARIO, graph: GRAPH, nodeId: "search", input: { text: "закреплено" } },
    });
  });

  test("retrying a step keeps the steps that already succeeded", async () => {
    const { state, emit, calls } = rig();
    await state.start(GRAPH);
    emit(STREAM, stepEvent("s1", "trigger", "succeeded"));
    emit(STREAM, stepEvent("s2", "search", "failed", { error: "сбой" }));
    emit(STREAM, { type: "end", outcome: { status: "failed", code: "RUN_FAILED" } });
    await state.retryStep("search");
    expect(calls[1]).toEqual({
      channel: "scenarios.retryStep",
      input: { runId: RUN, nodeId: "search" },
    });
    expect(state.view?.retryOf).toBe(RUN);
    expect(state.nodes.get("trigger")?.status).toBe("succeeded");
    expect(state.nodes.has("search")).toBe(false);
    emit(RETRY_STREAM, stepEvent("s3", "search", "running"));
    expect(state.nodes.get("search")?.status).toBe("running");
  });

  test("a refused start shows the host's reason", async () => {
    const { state, bridge } = rig();
    bridge.fail("scenarios.run", AppErrorCode.VALIDATION_FAILED, "Сценарий содержит ошибки");
    expect(await state.start(GRAPH)).toBe(false);
    expect(state.error).toBe("Сценарий содержит ошибки");
    expect(state.view).toBeNull();
  });
});

describe("run log strip height", () => {
  test("the height is clamped between the minimum and the room left for the canvas", () => {
    const { state } = rig();
    expect(state.stripHeight).toBe(STRIP_HEIGHT_DEFAULT);
    state.toggleStrip();
    state.setStripHeight(40);
    expect(state.stripHeight).toBe(STRIP_HEIGHT_MIN);
    expect(state.stripOpen).toBe(true);
    state.setStripHeight(900, 420);
    expect(state.stripHeight).toBe(420);
    state.setStripHeight(300.4, 420);
    expect(state.stripHeight).toBe(300);
  });
});
