import { describe, expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  contract,
  ScenarioGraphDto,
  type Contract,
  type ScenarioDetailDto,
  type ScenarioDraftDto,
  type ScenarioGraphDto as Graph,
  type ScenarioId,
  type ScenarioNodeDto,
  type ScenarioNodeTypeDto,
  type ScenarioVersionId,
  type ToolDto,
  type ToolId,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createFakeClock } from "../../../../test/helpers/fakeClock.ts";
import {
  applyAction,
  attachmentLinkId,
  catalogueOf,
  edgeLinkId,
  EMPTY_GRAPH,
  sameGraph,
  type GraphAction,
  type GraphResult,
} from "../../src/renderer/features/scenarios/graphModel.ts";
import {
  commit,
  HISTORY_LIMIT,
  redo,
  startHistory,
  undo,
} from "../../src/renderer/features/scenarios/history.ts";
import {
  nodeSize,
  slotOffset,
  slotUnder,
  tidy,
} from "../../src/renderer/features/scenarios/layout.ts";
import { paletteGroups } from "../../src/renderer/features/scenarios/palette.ts";
import {
  CONNECT_REFUSALS,
  TOOL_OFF,
  TOOL_UNAVAILABLE,
  TOOL_UNREGISTERED,
} from "../../src/renderer/features/scenarios/scenarioCopy.ts";
import ScenarioEditor, {
  slotHandle,
  type Timers,
} from "../../src/renderer/features/scenarios/ScenarioEditor.ts";
import { canvasCommand } from "../../src/renderer/features/scenarios/shortcuts.ts";

const SCENARIO = "0199aa11-1111-7111-8111-111111111111" as ScenarioId;
const VERSION = "0199aa11-2222-7111-8111-111111111111" as ScenarioVersionId;

function type(overrides: Partial<ScenarioNodeTypeDto> & { type: string }): ScenarioNodeTypeDto {
  return {
    role: "action",
    trigger: null,
    fills: [],
    slots: [],
    requiredParams: [],
    toolKey: null,
    ...overrides,
  };
}

const TYPES: ScenarioNodeTypeDto[] = [
  type({ type: "trigger.chat", role: "trigger", trigger: "chat" }),
  type({
    type: "agent.tools",
    slots: [
      { slot: "model", required: true, multiple: false },
      { slot: "memory", required: false, multiple: false },
      { slot: "tool", required: false, multiple: true },
    ],
  }),
  type({ type: "model.chat", role: "subNode", fills: ["model"], requiredParams: ["modelId"] }),
  type({ type: "memory.window", role: "subNode", fills: ["memory"] }),
  type({ type: "embedding.model", role: "subNode", fills: ["embedding"] }),
  type({
    type: "vector.search",
    fills: ["tool"],
    toolKey: "vector.search",
    slots: [{ slot: "embedding", required: true, multiple: false }],
  }),
  type({ type: "slack.post", toolKey: "slack.post", fills: ["tool"] }),
  type({ type: "fs.delete", toolKey: "fs.delete", fills: ["tool"] }),
  type({ type: "web.fetch", toolKey: "web.fetch", fills: ["tool"] }),
  type({ type: "ghost.tool", toolKey: "ghost.tool", fills: ["tool"] }),
];

const CATALOGUE = catalogueOf(TYPES);

function toolId(n: number): ToolId {
  return `0199cc11-4444-7111-8111-${String(n).padStart(12, "0")}` as ToolId;
}

function tool(n: number, key: string, overrides: Partial<ToolDto> = {}): ToolDto {
  return {
    id: toolId(n),
    key,
    source: "builtin",
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
    registeredAt: 1_000,
    updatedAt: 1_000,
    ...overrides,
  };
}

const TOOLS: ToolDto[] = [
  tool(1, "vector.search", { displayName: "Поиск по хранилищу" }),
  tool(2, "slack.post", { source: "integration", sourceRef: "slack", displayName: "Slack" }),
  tool(3, "fs.delete", { tier: "off", destructive: true }),
  tool(4, "web.fetch", { available: false }),
];

function node(id: string, nodeType: string, x = 0, y = 0): ScenarioNodeDto {
  return { id, type: nodeType, position: { x, y }, params: {} };
}

function graphOf(nodes: ScenarioNodeDto[], extra: Partial<Graph> = {}): Graph {
  return { nodes, edges: [], attachments: [], ...extra };
}

function run(graph: Graph, ...actions: GraphAction[]): GraphResult {
  let result: GraphResult = { ok: true, graph, created: [] };
  for (const action of actions) {
    if (!result.ok) return result;
    result = applyAction(result.graph, action, CATALOGUE);
  }
  return result;
}

function expectGraph(result: GraphResult): Graph {
  if (!result.ok) throw new Error(`refused: ${JSON.stringify(result.refusal)}`);
  return result.graph;
}

const MOCKUP = graphOf([
  node("trigger", "trigger.chat", 24, 268),
  node("agent", "agent.tools", 250, 258),
  node("model", "model.chat", 236, 418),
  node("memory", "memory.window", 388, 418),
  node("kb", "vector.search", 540, 258),
  node("embed", "embedding.model", 574, 418),
  node("slack", "slack.post", 250, 120),
]);

function mockupGraph(): Graph {
  return expectGraph(
    run(
      MOCKUP,
      { type: "connect", from: "trigger", to: "agent" },
      { type: "connect", from: "trigger", to: "slack" },
      { type: "attach", parentId: "agent", childId: "model", slot: "model" },
      { type: "attach", parentId: "agent", childId: "memory", slot: "memory" },
      { type: "attach", parentId: "agent", childId: "kb", slot: "tool" },
      { type: "attach", parentId: "kb", childId: "embed", slot: "embedding" },
    ),
  );
}

describe("graph reducers", () => {
  test("adding a node appends it and reports its id, and an existing id is left alone", () => {
    const added = run(EMPTY_GRAPH, { type: "addNode", node: node("trigger", "trigger.chat") });
    expect(added.ok && added.created).toEqual(["trigger"]);
    const again = run(expectGraph(added), {
      type: "addNode",
      node: node("trigger", "agent.tools"),
    });
    expect(expectGraph(again).nodes).toHaveLength(1);
  });

  test("the mockup graph builds by hand and round-trips through the contract schema", () => {
    const graph = mockupGraph();
    expect(graph.edges).toHaveLength(2);
    expect(graph.attachments.map((entry) => entry.slot)).toEqual([
      "model",
      "memory",
      "tool",
      "embedding",
    ]);
    const wire = JSON.parse(JSON.stringify(graph)) as unknown;
    expect(ScenarioGraphDto.parse(wire)).toEqual(graph);
  });

  test("deleting a node takes its edges and attachments with it", () => {
    const graph = expectGraph(run(mockupGraph(), { type: "remove", nodeIds: ["kb"], linkIds: [] }));
    expect(graph.nodes.map((entry) => entry.id)).not.toContain("kb");
    expect(graph.attachments.some((entry) => entry.childNodeId === "kb")).toBe(false);
    expect(graph.attachments.some((entry) => entry.parentNodeId === "kb")).toBe(false);
    expect(graph.nodes.map((entry) => entry.id)).toContain("embed");
  });

  test("deleting a link removes only that link", () => {
    const graph = mockupGraph();
    const [edge] = graph.edges;
    const [attachment] = graph.attachments;
    if (edge === undefined || attachment === undefined) throw new Error("fixture");
    const next = expectGraph(
      run(graph, {
        type: "remove",
        nodeIds: [],
        linkIds: [edgeLinkId(edge), attachmentLinkId(attachment)],
      }),
    );
    expect(next.nodes).toHaveLength(graph.nodes.length);
    expect(next.edges).toHaveLength(graph.edges.length - 1);
    expect(next.attachments).toHaveLength(graph.attachments.length - 1);
  });

  test("connecting refuses self loops, cycles, edges into triggers, sub-nodes and repeats", () => {
    const graph = mockupGraph();
    const refusal = (from: string, to: string) => {
      const result = run(graph, { type: "connect", from, to });
      return result.ok ? null : result.refusal;
    };
    expect(refusal("agent", "agent")).toEqual({ kind: "connect", reason: "selfLoop" });
    expect(refusal("slack", "trigger")).toEqual({ kind: "connect", reason: "intoTrigger" });
    expect(refusal("agent", "model")).toEqual({ kind: "connect", reason: "notTrunk" });
    expect(refusal("agent", "kb")).toEqual({ kind: "connect", reason: "notTrunk" });
    expect(refusal("trigger", "agent")).toEqual({ kind: "connect", reason: "duplicate" });
    const chained = expectGraph(run(graph, { type: "connect", from: "agent", to: "slack" }));
    const cycle = applyAction(chained, { type: "connect", from: "slack", to: "agent" }, CATALOGUE);
    expect(cycle.ok ? null : cycle.refusal).toEqual({ kind: "connect", reason: "cycle" });
    expect(Object.keys(CONNECT_REFUSALS).sort()).toEqual(
      ["cycle", "duplicate", "intoTrigger", "notTrunk", "selfLoop"].sort(),
    );
  });

  test("re-routing moves an edge to a new target and refuses one that would be invalid", () => {
    const graph = mockupGraph();
    const edge = graph.edges.find((entry) => entry.to === "slack");
    if (edge === undefined) throw new Error("fixture");
    const moved = expectGraph(
      run(graph, { type: "reroute", linkId: edgeLinkId(edge), from: "agent", to: "slack" }),
    );
    expect(moved.edges.some((entry) => entry.from === "agent" && entry.to === "slack")).toBe(true);
    expect(moved.edges.some((entry) => entry.from === "trigger" && entry.to === "slack")).toBe(
      false,
    );
    const refused = run(graph, {
      type: "reroute",
      linkId: edgeLinkId(edge),
      from: "trigger",
      to: "trigger",
    });
    expect(refused.ok).toBe(false);
  });

  test("duplicating copies nodes with fresh ids, offset positions and the links among them", () => {
    const graph = mockupGraph();
    const result = run(graph, { type: "duplicate", nodeIds: ["agent", "model", "slack"] });
    const next = expectGraph(result);
    expect(result.ok && result.created).toEqual(["tools-1", "chat-1", "post-1"]);
    const copy = next.nodes.find((entry) => entry.id === "tools-1");
    expect(copy?.position).toEqual({ x: 282, y: 290 });
    expect(next.attachments).toContainEqual({
      parentNodeId: "tools-1",
      childNodeId: "chat-1",
      slot: "model",
    });
    expect(next.edges).toHaveLength(graph.edges.length);
  });

  test("updating a node sets and clears its name, params and disabled flag", () => {
    const named = expectGraph(
      run(mockupGraph(), {
        type: "updateNode",
        nodeId: "model",
        patch: { name: "  Ollama Chat ", params: { modelId: "gpt-oss:120b" }, disabled: true },
      }),
    );
    const model = named.nodes.find((entry) => entry.id === "model");
    expect(model).toMatchObject({ name: "Ollama Chat", params: { modelId: "gpt-oss:120b" } });
    expect(model?.disabled).toBe(true);
    const cleared = expectGraph(
      run(named, { type: "updateNode", nodeId: "model", patch: { name: " ", disabled: false } }),
    );
    const plain = cleared.nodes.find((entry) => entry.id === "model");
    expect(plain && "name" in plain).toBe(false);
    expect(plain && "disabled" in plain).toBe(false);
  });
});

describe("attachment compatibility", () => {
  const base = expectGraph(
    run(
      graphOf([
        node("trigger", "trigger.chat"),
        node("agent", "agent.tools"),
        node("other", "agent.tools"),
        node("model", "model.chat"),
        node("model2", "model.chat"),
        node("memory", "memory.window"),
        node("kb", "vector.search"),
        node("embed", "embedding.model"),
        node("slack", "slack.post"),
      ]),
      { type: "connect", from: "trigger", to: "agent" },
      { type: "attach", parentId: "agent", childId: "model", slot: "model" },
    ),
  );

  const reason = (
    parentId: string,
    childId: string,
    slot: "model" | "memory" | "tool" | "embedding",
  ) => {
    const result = run(base, { type: "attach", parentId, childId, slot });
    return result.ok ? null : result.refusal.kind === "attach" ? result.refusal.reason : null;
  };

  test("a node fills only the slots its type declares", () => {
    expect(reason("agent", "memory", "model")).toBe("wrongKind");
    expect(reason("agent", "memory", "memory")).toBeNull();
    expect(reason("agent", "kb", "tool")).toBeNull();
  });

  test("a parent without the slot, a full single slot and a self drop are refused", () => {
    expect(reason("slack", "embed", "embedding")).toBe("noSlot");
    expect(reason("agent", "model2", "model")).toBe("occupied");
    expect(reason("agent", "agent", "model")).toBe("self");
  });

  test("a step joined by main edges is refused until those edges are gone", () => {
    const connected = expectGraph(run(base, { type: "connect", from: "agent", to: "slack" }));
    const refused = run(connected, {
      type: "attach",
      parentId: "other",
      childId: "slack",
      slot: "tool",
    });
    expect(refused.ok ? null : refused.refusal).toEqual({
      kind: "attach",
      reason: "hasEdges",
      slot: "tool",
      childId: "slack",
      parentId: "other",
    });
  });

  test("a sub-node cannot hang below its own descendant", () => {
    const nested = expectGraph(
      run(base, { type: "attach", parentId: "agent", childId: "kb", slot: "tool" }),
    );
    const looped = applyAction(
      { ...nested, nodes: [...nested.nodes, node("agent2", "agent.tools")] },
      { type: "attach", parentId: "kb", childId: "agent", slot: "embedding" },
      CATALOGUE,
    );
    expect(looped.ok).toBe(false);
  });

  test("the tool slot takes many nodes, and re-attaching moves a node instead of copying it", () => {
    const tools = expectGraph(
      run(
        base,
        { type: "attach", parentId: "agent", childId: "kb", slot: "tool" },
        { type: "attach", parentId: "agent", childId: "slack", slot: "tool" },
      ),
    );
    expect(tools.attachments.filter((entry) => entry.slot === "tool")).toHaveLength(2);
    const moved = expectGraph(
      run(tools, { type: "attach", parentId: "other", childId: "slack", slot: "tool" }),
    );
    expect(moved.attachments.filter((entry) => entry.childNodeId === "slack")).toEqual([
      { parentNodeId: "other", childNodeId: "slack", slot: "tool" },
    ]);
  });

  test("a node dragged near a slot targets it and carries the reason when it does not fit", () => {
    const agent = base.nodes.find((entry) => entry.id === "agent");
    const spec = CATALOGUE.get("agent.tools");
    if (agent === undefined || spec === undefined) throw new Error("fixture");
    const memorySlot = slotOffset(spec, "memory") ?? 0;
    const below = agent.position.y + nodeSize(spec).height;
    const childWidth = nodeSize(CATALOGUE.get("memory.window")).width;
    const at = { x: agent.position.x + memorySlot - childWidth / 2, y: below + 6 };
    expect(slotUnder(base, CATALOGUE, "memory", at)).toEqual({
      parentId: "agent",
      slot: "memory",
      refusal: null,
    });
    expect(slotUnder(base, CATALOGUE, "embed", at)).toEqual({
      parentId: "agent",
      slot: "memory",
      refusal: "wrongKind",
    });
    expect(slotUnder(base, CATALOGUE, "memory", { x: at.x + 400, y: at.y })).toBeNull();
  });
});

describe("history", () => {
  test("undo and redo walk the committed graphs and a new commit drops the redo branch", () => {
    const first = graphOf([node("a", "trigger.chat")]);
    const second = graphOf([node("a", "trigger.chat"), node("b", "agent.tools")]);
    const third = graphOf([node("c", "trigger.chat")]);
    let history = commit(commit(startHistory(EMPTY_GRAPH), first), second);
    history = undo(history);
    expect(history.present).toBe(first);
    history = redo(history);
    expect(history.present).toBe(second);
    history = commit(undo(history), third);
    expect(history.present).toBe(third);
    expect(redo(history)).toBe(history);
    expect(undo(undo(undo(history))).present).toBe(EMPTY_GRAPH);
  });

  test("the history keeps a bounded number of steps", () => {
    let history = startHistory(EMPTY_GRAPH);
    for (let index = 0; index < HISTORY_LIMIT + 20; index += 1) {
      history = commit(history, graphOf([node(`n${String(index)}`, "trigger.chat")]));
    }
    expect(history.past).toHaveLength(HISTORY_LIMIT);
    expect(commit(history, history.present)).toBe(history);
  });
});

describe("tidy", () => {
  test("steps line up by depth and sub-nodes hang below their parent", () => {
    const tidied = tidy(mockupGraph(), CATALOGUE);
    const at = (id: string) => tidied.nodes.find((entry) => entry.id === id)?.position;
    const trigger = at("trigger");
    const agent = at("agent");
    const model = at("model");
    const embed = at("embed");
    const kb = at("kb");
    if (!trigger || !agent || !model || !embed || !kb) throw new Error("positions");
    expect(agent.x).toBeGreaterThan(trigger.x);
    expect(at("slack")?.x).toBe(agent.x);
    expect(model.y).toBeGreaterThan(agent.y);
    expect(embed.y).toBeGreaterThan(kb.y);
  });
});

describe("palette", () => {
  test("node types group by source and a tool switched off is shown disabled with the reason", () => {
    const groups = paletteGroups(TYPES, TOOLS, [], "");
    expect(groups.map((group) => group.key).slice(0, 2)).toEqual(["triggers", "agents"]);
    const entries = groups.flatMap((group) => group.entries);
    const reason = (type: string) => entries.find((entry) => entry.type === type)?.disabledReason;
    expect(reason("fs.delete")).toBe(TOOL_OFF);
    expect(reason("web.fetch")).toBe(TOOL_UNAVAILABLE);
    expect(reason("ghost.tool")).toBe(TOOL_UNREGISTERED);
    expect(reason("vector.search")).toBeNull();
    expect(reason("agent.tools")).toBeNull();
    expect(entries.find((entry) => entry.type === "vector.search")?.label).toBe(
      "Поиск по хранилищу",
    );
    const integration = groups.find((group) => group.key === "tool:integration:slack");
    expect(integration?.entries.map((entry) => entry.type)).toEqual(["slack.post"]);
  });

  test("a disabled tool loses the off reason once it is switched back on", () => {
    const enabled = TOOLS.map((entry) =>
      entry.key === "fs.delete" ? { ...entry, tier: "ask" as const } : entry,
    );
    const entries = paletteGroups(TYPES, enabled, [], "").flatMap((group) => group.entries);
    expect(entries.find((entry) => entry.type === "fs.delete")?.disabledReason).toBeNull();
  });

  test("the search matches the label and the type", () => {
    const found = paletteGroups(TYPES, TOOLS, [], "хранилищ").flatMap((group) => group.entries);
    expect(found.map((entry) => entry.type)).toEqual(["vector.search"]);
    expect(paletteGroups(TYPES, TOOLS, [], "model.chat")[0]?.entries[0]?.type).toBe("model.chat");
  });
});

describe("keyboard", () => {
  const press = (
    key: string,
    mods: Partial<Record<"ctrl" | "meta" | "shift" | "alt", boolean>> = {},
  ) =>
    canvasCommand({
      key,
      ctrlKey: mods.ctrl ?? false,
      metaKey: mods.meta ?? false,
      shiftKey: mods.shift ?? false,
      altKey: mods.alt ?? false,
    });

  test("editing shortcuts map to canvas commands", () => {
    expect(press("Delete")).toBe("delete");
    expect(press("Backspace")).toBe("delete");
    expect(press("Escape")).toBe("deselect");
    expect(press("d", { ctrl: true })).toBe("duplicate");
    expect(press("a", { meta: true })).toBe("selectAll");
    expect(press("z", { ctrl: true })).toBe("undo");
    expect(press("Z", { ctrl: true, shift: true })).toBe("redo");
    expect(press("y", { ctrl: true })).toBe("redo");
    expect(press("d")).toBeNull();
    expect(press("z", { ctrl: true, alt: true })).toBeNull();
  });
});

interface Pending {
  readonly callback: () => void;
  readonly ms: number;
}

function manualTimers() {
  const pending = new Map<number, Pending>();
  let next = 0;
  const timers: Timers = {
    set: (callback, ms) => {
      next += 1;
      pending.set(next, { callback, ms });
      return next;
    },
    clear: (handle) => {
      pending.delete(handle as number);
    },
  };
  return {
    timers,
    get size() {
      return pending.size;
    },
    flush() {
      const due = [...pending.values()];
      pending.clear();
      for (const entry of due) entry.callback();
    },
  };
}

function detail(graph: Graph, version = 1): ScenarioDetailDto {
  return {
    id: SCENARIO,
    name: "Триаж",
    tags: ["support"],
    active: false,
    currentVersionId: VERSION,
    version,
    trigger: null,
    issueCount: 0,
    createdAt: 1_000,
    updatedAt: 1_000,
    graph,
    issues: [],
  };
}

async function settle() {
  for (let index = 0; index < 5; index += 1) await Promise.resolve();
}

function setup(options: { draft?: ScenarioDraftDto | null; saved?: Graph } = {}) {
  const bridge = createFakeBridge<Contract>();
  const clock = createFakeClock();
  const timers = manualTimers();
  const drafts: Graph[] = [];
  const updates: unknown[] = [];
  let saved = options.saved ?? MOCKUP;
  bridge
    .handle("scenarios.get", () => detail(saved))
    .handle("scenarios.draft", () => options.draft ?? null)
    .handle("scenarios.catalogue", () => TYPES)
    .handle("tools.list", () => TOOLS)
    .handle("mcp.list", () => [])
    .handle("scenarios.validate", ({ graph }) =>
      graph.nodes.some((entry) => entry.type === "trigger.chat")
        ? { valid: true, issues: [] }
        : { valid: false, issues: [{ rule: "noTrigger" }] },
    )
    .handle("scenarios.saveDraft", ({ id, graph }) => {
      drafts.push(graph);
      return { scenarioId: id, graph, savedAt: clock() };
    })
    .handle("scenarios.discardDraft", ({ id }) => ({ id }))
    .handle("scenarios.update", (input) => {
      updates.push(input);
      saved = input.graph ?? saved;
      return detail(saved, 2);
    });
  const editor = new ScenarioEditor(createIpcClient(contract, bridge), SCENARIO, {
    clock,
    timers: timers.timers,
  });
  return { bridge, clock, timers, editor, drafts, updates };
}

describe("scenario editor", () => {
  test("a draft newer than the saved version is restored on load", async () => {
    const draftGraph = mockupGraph();
    const { editor } = setup({
      draft: { scenarioId: SCENARIO, graph: draftGraph, savedAt: 1_700_000_000_000 },
    });
    await editor.load();
    expect(editor.graph).toEqual(draftGraph);
    expect(editor.dirty).toBe(true);
    expect(editor.notice).not.toBeNull();
  });

  test("mutations autosave a draft after a pause and validation follows the graph", async () => {
    const { editor, timers, drafts } = setup();
    await editor.load();
    expect(editor.dirty).toBe(false);
    editor.dispatch({ type: "remove", nodeIds: ["trigger"], linkIds: [] });
    editor.dispatch({ type: "connect", from: "agent", to: "slack" });
    expect(drafts).toHaveLength(0);
    expect(editor.draftState).toBe("pending");
    timers.flush();
    await settle();
    expect(drafts).toHaveLength(1);
    expect(drafts[0]).toEqual(editor.graph);
    expect(editor.draftState).toBe("saved");
    expect(editor.issues).toEqual([{ rule: "noTrigger" }]);
  });

  test("saving creates a version from the working graph and clears the dirty state", async () => {
    const { editor, updates } = setup();
    await editor.load();
    editor.dispatch({ type: "connect", from: "trigger", to: "agent" });
    expect(await editor.save("  Связь с агентом ")).toBe(true);
    expect(updates).toEqual([{ id: SCENARIO, graph: editor.graph, note: "Связь с агентом" }]);
    expect(editor.dirty).toBe(false);
    expect(editor.scenario?.version).toBe(2);
    expect(editor.draftState).toBe("clean");
  });

  test("a saved graph echoed back in host key order is not reported as changed", async () => {
    const { editor, bridge } = setup();
    await editor.load();
    editor.updateNode("model", { name: "Ollama Chat", params: { modelId: "m", providerId: "p" } });
    const hostOrder = (graph: Graph): Graph => ({
      nodes: graph.nodes.map((entry) => {
        const params = Object.fromEntries(Object.entries(entry.params).reverse());
        return entry.name === undefined
          ? { id: entry.id, type: entry.type, position: entry.position, params }
          : { id: entry.id, type: entry.type, name: entry.name, position: entry.position, params };
      }),
      edges: graph.edges,
      attachments: graph.attachments,
    });
    bridge.handle("scenarios.update", (input) => detail(hostOrder(input.graph ?? MOCKUP), 2));
    expect(sameGraph(hostOrder(editor.graph), editor.graph)).toBe(true);
    expect(JSON.stringify(hostOrder(editor.graph))).not.toBe(JSON.stringify(editor.graph));
    await editor.save();
    expect(editor.dirty).toBe(false);
    expect(editor.draftState).toBe("clean");
  });

  test("dropping a sub-node on a slot attaches it in one undoable step", async () => {
    const { editor } = setup();
    await editor.load();
    const agentSpec = CATALOGUE.get("agent.tools");
    if (agentSpec === undefined) throw new Error("fixture");
    const agent = editor.graph.nodes.find((entry) => entry.id === "agent");
    if (agent === undefined) throw new Error("fixture");
    const offset = slotOffset(agentSpec, "model") ?? 0;
    const drop = {
      x: agent.position.x + offset - nodeSize(CATALOGUE.get("model.chat")).width / 2,
      y: agent.position.y + nodeSize(agentSpec).height + 10,
    };
    editor.dragNodes({ model: drop });
    expect(editor.dropTarget).toEqual({ parentId: "agent", slot: "model", refusal: null });
    editor.endDrag({ model: drop });
    expect(editor.dropTarget).toBeNull();
    expect(editor.graph.attachments).toEqual([
      { parentNodeId: "agent", childNodeId: "model", slot: "model" },
    ]);
    expect(editor.graph.nodes.find((entry) => entry.id === "model")?.position).toEqual(drop);
    editor.undo();
    expect(editor.graph).toEqual(MOCKUP);
  });

  test("an incompatible drop still moves the node but is refused with a visible reason", async () => {
    const { editor } = setup();
    await editor.load();
    const agentSpec = CATALOGUE.get("agent.tools");
    const agent = editor.graph.nodes.find((entry) => entry.id === "agent");
    if (agentSpec === undefined || agent === undefined) throw new Error("fixture");
    const drop = {
      x: agent.position.x + (slotOffset(agentSpec, "model") ?? 0) - 72,
      y: agent.position.y + nodeSize(agentSpec).height,
    };
    editor.endDrag({ memory: drop });
    expect(editor.graph.attachments).toEqual([]);
    expect(editor.graph.nodes.find((entry) => entry.id === "memory")?.position).toEqual(drop);
    expect(editor.refusal).not.toBeNull();
  });

  test("connecting from a slot port attaches instead of drawing a main edge", async () => {
    const { editor } = setup();
    await editor.load();
    expect(editor.connect("agent", slotHandle("tool"), "kb", "up")).toBe(true);
    expect(editor.connect("embed", "up", "kb", slotHandle("embedding"))).toBe(true);
    expect(editor.graph.edges).toEqual([]);
    expect(editor.graph.attachments).toEqual([
      { parentNodeId: "agent", childNodeId: "kb", slot: "tool" },
      { parentNodeId: "kb", childNodeId: "embed", slot: "embedding" },
    ]);
    expect(editor.connect("agent", "out", "agent", "in")).toBe(false);
    expect(editor.refusal).toBe(CONNECT_REFUSALS.selfLoop);
  });

  test("adding a step after a selected one places it to the right and connects it", async () => {
    const { editor } = setup({ saved: graphOf([node("trigger", "trigger.chat", 40, 40)]) });
    await editor.load();
    editor.select("trigger");
    expect(editor.addStep("agent.tools", { x: 0, y: 0 })).toBe(true);
    expect(editor.graph.edges).toEqual([
      { from: "trigger", to: "tools-1", fromPort: "main", toPort: "main" },
    ]);
    expect(editor.selection.nodes).toEqual(["tools-1"]);
    expect(editor.addStep("model.chat", { x: 0, y: 0 })).toBe(true);
    expect(editor.graph.attachments).toEqual([
      { parentNodeId: "tools-1", childNodeId: "chat-1", slot: "model" },
    ]);
    expect(editor.addStep("fs.delete", { x: 0, y: 0 })).toBe(false);
    expect(editor.refusal).toBe(TOOL_OFF);
  });

  test("selection commands delete, duplicate and select everything", async () => {
    const { editor } = setup();
    await editor.load();
    editor.selectAll();
    expect(editor.selection.nodes).toHaveLength(MOCKUP.nodes.length);
    editor.setSelection({ nodes: ["slack"], links: [] });
    editor.duplicateSelection();
    expect(editor.selection.nodes).toEqual(["post-1"]);
    editor.deleteSelection();
    expect(editor.graph.nodes.map((entry) => entry.id)).not.toContain("post-1");
    expect(editor.selection.nodes).toEqual([]);
    expect(editor.canUndo).toBe(true);
    editor.undo();
    editor.undo();
    expect(editor.canUndo).toBe(false);
    expect(editor.canRedo).toBe(true);
  });

  test("activation refused by the host shows its message and the issues it names", async () => {
    const { editor, bridge } = setup();
    await editor.load();
    bridge.fail("scenarios.activate", AppErrorCode.VALIDATION_FAILED, "Сценарий содержит ошибки");
    await editor.setActive(true);
    expect(editor.error).toBe("Сценарий содержит ошибки");
    expect(editor.scenario?.active).toBe(false);
  });

  test("discarding the draft returns to the saved version", async () => {
    const { editor, bridge } = setup({
      draft: { scenarioId: SCENARIO, graph: mockupGraph(), savedAt: 1_700_000_000_000 },
    });
    await editor.load();
    await editor.discardDraft();
    expect(editor.graph).toEqual(MOCKUP);
    expect(editor.dirty).toBe(false);
    expect(bridge.calls.some((call) => call.channel === "scenarios.discardDraft")).toBe(true);
  });
});
