import {
  SCENARIO_MAIN_PORT,
  SCENARIO_NODE_ID_MAX,
  type Json,
  type ScenarioAttachmentDto,
  type ScenarioEdgeDto,
  type ScenarioGraphDto,
  type ScenarioNodeDto,
  type ScenarioNodeSettingsDto,
  type ScenarioNodeTypeDto,
  type ScenarioParams,
  type ScenarioPositionDto,
  type ScenarioSlot,
} from "@zvs/shared";

export type Catalogue = ReadonlyMap<string, ScenarioNodeTypeDto>;

export const EMPTY_GRAPH: ScenarioGraphDto = { nodes: [], edges: [], attachments: [] };

export const DUPLICATE_OFFSET = 32;

export type ConnectRefusal = "selfLoop" | "notTrunk" | "intoTrigger" | "duplicate" | "cycle";

export type AttachRefusal =
  "self" | "unknownType" | "noSlot" | "wrongKind" | "occupied" | "hasEdges" | "cycle";

export type Refusal =
  | { readonly kind: "connect"; readonly reason: ConnectRefusal }
  | {
      readonly kind: "attach";
      readonly reason: AttachRefusal;
      readonly slot: ScenarioSlot;
      readonly childId: string;
      readonly parentId: string;
    };

export interface NodePatch {
  readonly name?: string;
  readonly params?: ScenarioParams;
  readonly disabled?: boolean;
  readonly settings?: ScenarioNodeSettingsDto | null;
  readonly notes?: string;
  readonly pinnedInput?: Json | null;
}

function isEmptySettings(settings: ScenarioNodeSettingsDto): boolean {
  return Object.values(settings).every((value) => value === undefined);
}

export type GraphAction =
  | { readonly type: "addNode"; readonly node: ScenarioNodeDto }
  | {
      readonly type: "moveNodes";
      readonly positions: Readonly<Record<string, ScenarioPositionDto>>;
    }
  | {
      readonly type: "remove";
      readonly nodeIds: readonly string[];
      readonly linkIds: readonly string[];
    }
  | { readonly type: "connect"; readonly from: string; readonly to: string }
  | {
      readonly type: "reroute";
      readonly linkId: string;
      readonly from: string;
      readonly to: string;
    }
  | {
      readonly type: "attach";
      readonly parentId: string;
      readonly childId: string;
      readonly slot: ScenarioSlot;
    }
  | { readonly type: "duplicate"; readonly nodeIds: readonly string[] }
  | { readonly type: "updateNode"; readonly nodeId: string; readonly patch: NodePatch }
  | { readonly type: "replace"; readonly graph: ScenarioGraphDto };

export type GraphResult =
  | { readonly ok: true; readonly graph: ScenarioGraphDto; readonly created: readonly string[] }
  | { readonly ok: false; readonly refusal: Refusal };

function canonical(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonical);
  if (value !== null && typeof value === "object") {
    const record = value as Record<string, unknown>;
    return Object.fromEntries(
      Object.keys(record)
        .sort()
        .map((key) => [key, canonical(record[key])]),
    );
  }
  return value;
}

export function sameGraph(left: ScenarioGraphDto, right: ScenarioGraphDto): boolean {
  return left === right || JSON.stringify(canonical(left)) === JSON.stringify(canonical(right));
}

export function catalogueOf(types: readonly ScenarioNodeTypeDto[]): Catalogue {
  return new Map(types.map((entry) => [entry.type, entry]));
}

export function edgeLinkId(edge: ScenarioEdgeDto): string {
  return `e:${JSON.stringify([edge.from, edge.fromPort, edge.to, edge.toPort])}`;
}

export function attachmentLinkId(attachment: ScenarioAttachmentDto): string {
  return `a:${JSON.stringify([attachment.parentNodeId, attachment.slot, attachment.childNodeId])}`;
}

export function findNode(graph: ScenarioGraphDto, id: string): ScenarioNodeDto | undefined {
  return graph.nodes.find((node) => node.id === id);
}

export function isTrunk(catalogue: Catalogue, node: ScenarioNodeDto | undefined): boolean {
  if (node === undefined) return false;
  const spec = catalogue.get(node.type);
  return spec === undefined || spec.role !== "subNode";
}

export function parentOf(
  graph: ScenarioGraphDto,
  childId: string,
): ScenarioAttachmentDto | undefined {
  return graph.attachments.find((attachment) => attachment.childNodeId === childId);
}

export function childrenIn(
  graph: ScenarioGraphDto,
  parentId: string,
  slot: ScenarioSlot,
): ScenarioAttachmentDto[] {
  return graph.attachments.filter(
    (attachment) => attachment.parentNodeId === parentId && attachment.slot === slot,
  );
}

function isMainCapable(catalogue: Catalogue, graph: ScenarioGraphDto, id: string): boolean {
  return isTrunk(catalogue, findNode(graph, id)) && parentOf(graph, id) === undefined;
}

function reaches(graph: ScenarioGraphDto, from: string, target: string): boolean {
  const seen = new Set<string>();
  const queue = [from];
  while (queue.length > 0) {
    const current = queue.shift();
    if (current === undefined) break;
    if (current === target) return true;
    if (seen.has(current)) continue;
    seen.add(current);
    for (const edge of graph.edges) if (edge.from === current) queue.push(edge.to);
  }
  return false;
}

export function connectRefusal(
  graph: ScenarioGraphDto,
  catalogue: Catalogue,
  from: string,
  to: string,
  ignore: string | null = null,
): ConnectRefusal | null {
  if (from === to) return "selfLoop";
  if (!isMainCapable(catalogue, graph, from) || !isMainCapable(catalogue, graph, to)) {
    return "notTrunk";
  }
  const target = findNode(graph, to);
  if (target !== undefined && catalogue.get(target.type)?.role === "trigger") return "intoTrigger";
  const edges = graph.edges.filter((edge) => edgeLinkId(edge) !== ignore);
  if (edges.some((edge) => edge.from === from && edge.to === to)) return "duplicate";
  if (reaches({ ...graph, edges }, to, from)) return "cycle";
  return null;
}

function attachmentAncestors(graph: ScenarioGraphDto, id: string): Set<string> {
  const ancestors = new Set<string>();
  let current = parentOf(graph, id);
  while (current !== undefined && !ancestors.has(current.parentNodeId)) {
    ancestors.add(current.parentNodeId);
    current = parentOf(graph, current.parentNodeId);
  }
  return ancestors;
}

export function attachRefusal(
  graph: ScenarioGraphDto,
  catalogue: Catalogue,
  parentId: string,
  childId: string,
  slot: ScenarioSlot,
): AttachRefusal | null {
  if (parentId === childId) return "self";
  const parent = findNode(graph, parentId);
  const child = findNode(graph, childId);
  const parentSpec = parent === undefined ? undefined : catalogue.get(parent.type);
  const childSpec = child === undefined ? undefined : catalogue.get(child.type);
  if (parentSpec === undefined || childSpec === undefined) return "unknownType";
  const slotSpec = parentSpec.slots.find((spec) => spec.slot === slot);
  if (slotSpec === undefined) return "noSlot";
  if (!childSpec.fills.includes(slot)) return "wrongKind";
  if (graph.edges.some((edge) => edge.from === childId || edge.to === childId)) return "hasEdges";
  if (attachmentAncestors(graph, parentId).has(childId)) return "cycle";
  const occupants = childrenIn(graph, parentId, slot).filter(
    (attachment) => attachment.childNodeId !== childId,
  );
  if (!slotSpec.multiple && occupants.length > 0) return "occupied";
  return null;
}

export function freshNodeId(graph: ScenarioGraphDto, type: string, taken: ReadonlySet<string>) {
  const tail = type.split(".").pop() ?? "node";
  const base = tail.replace(/[^a-zA-Z0-9_-]/g, "").slice(0, SCENARIO_NODE_ID_MAX - 6) || "node";
  const used = new Set([...graph.nodes.map((node) => node.id), ...taken]);
  for (let index = 1; ; index += 1) {
    const candidate = `${base}-${String(index)}`;
    if (!used.has(candidate)) return candidate;
  }
}

function ok(graph: ScenarioGraphDto, created: readonly string[] = []): GraphResult {
  return { ok: true, graph, created };
}

function removeLinks(graph: ScenarioGraphDto, linkIds: ReadonlySet<string>): ScenarioGraphDto {
  return {
    ...graph,
    edges: graph.edges.filter((edge) => !linkIds.has(edgeLinkId(edge))),
    attachments: graph.attachments.filter(
      (attachment) => !linkIds.has(attachmentLinkId(attachment)),
    ),
  };
}

function removeNodes(graph: ScenarioGraphDto, ids: ReadonlySet<string>): ScenarioGraphDto {
  return {
    nodes: graph.nodes.filter((node) => !ids.has(node.id)),
    edges: graph.edges.filter((edge) => !ids.has(edge.from) && !ids.has(edge.to)),
    attachments: graph.attachments.filter(
      (attachment) => !ids.has(attachment.parentNodeId) && !ids.has(attachment.childNodeId),
    ),
  };
}

function cloneParams(params: ScenarioParams): ScenarioParams {
  return JSON.parse(JSON.stringify(params)) as ScenarioParams;
}

function duplicate(graph: ScenarioGraphDto, ids: readonly string[]): GraphResult {
  const chosen = new Set(ids.filter((id) => findNode(graph, id) !== undefined));
  if (chosen.size === 0) return ok(graph);
  const renamed = new Map<string, string>();
  const taken = new Set<string>();
  for (const node of graph.nodes) {
    if (!chosen.has(node.id)) continue;
    const id = freshNodeId(graph, node.type, taken);
    taken.add(id);
    renamed.set(node.id, id);
  }
  const copies = graph.nodes
    .filter((node) => chosen.has(node.id))
    .map((node) => ({
      ...node,
      id: renamed.get(node.id) ?? node.id,
      params: cloneParams(node.params),
      position: { x: node.position.x + DUPLICATE_OFFSET, y: node.position.y + DUPLICATE_OFFSET },
    }));
  const edges = graph.edges
    .filter((edge) => chosen.has(edge.from) && chosen.has(edge.to))
    .map((edge) => ({
      ...edge,
      from: renamed.get(edge.from) ?? edge.from,
      to: renamed.get(edge.to) ?? edge.to,
    }));
  const attachments = graph.attachments
    .filter(
      (attachment) => chosen.has(attachment.parentNodeId) && chosen.has(attachment.childNodeId),
    )
    .map((attachment) => ({
      ...attachment,
      parentNodeId: renamed.get(attachment.parentNodeId) ?? attachment.parentNodeId,
      childNodeId: renamed.get(attachment.childNodeId) ?? attachment.childNodeId,
    }));
  return ok(
    {
      nodes: [...graph.nodes, ...copies],
      edges: [...graph.edges, ...edges],
      attachments: [...graph.attachments, ...attachments],
    },
    copies.map((node) => node.id),
  );
}

function mainEdge(from: string, to: string): ScenarioEdgeDto {
  return { from, to, fromPort: SCENARIO_MAIN_PORT, toPort: SCENARIO_MAIN_PORT };
}

function attach(
  graph: ScenarioGraphDto,
  catalogue: Catalogue,
  parentId: string,
  childId: string,
  slot: ScenarioSlot,
): GraphResult {
  const reason = attachRefusal(graph, catalogue, parentId, childId, slot);
  if (reason !== null) {
    return { ok: false, refusal: { kind: "attach", reason, slot, childId, parentId } };
  }
  const current = parentOf(graph, childId);
  if (current?.parentNodeId === parentId && current.slot === slot) return ok(graph);
  return ok({
    ...graph,
    attachments: [
      ...graph.attachments.filter((attachment) => attachment.childNodeId !== childId),
      { parentNodeId: parentId, childNodeId: childId, slot },
    ],
  });
}

function reroute(
  graph: ScenarioGraphDto,
  catalogue: Catalogue,
  linkId: string,
  from: string,
  to: string,
): GraphResult {
  const existing = graph.edges.find((edge) => edgeLinkId(edge) === linkId);
  if (existing === undefined) return ok(graph);
  if (existing.from === from && existing.to === to) return ok(graph);
  const reason = connectRefusal(graph, catalogue, from, to, linkId);
  if (reason !== null) return { ok: false, refusal: { kind: "connect", reason } };
  return ok({
    ...graph,
    edges: graph.edges.map((edge) => (edgeLinkId(edge) === linkId ? { ...edge, from, to } : edge)),
  });
}

function updateNode(graph: ScenarioGraphDto, nodeId: string, patch: NodePatch): GraphResult {
  return ok({
    ...graph,
    nodes: graph.nodes.map((node) => {
      if (node.id !== nodeId) return node;
      const next: ScenarioNodeDto = { ...node };
      if (patch.params !== undefined) next.params = patch.params;
      if (patch.name !== undefined) {
        const name = patch.name.trim();
        if (name.length === 0) delete next.name;
        else next.name = name;
      }
      if (patch.disabled !== undefined) {
        if (patch.disabled) next.disabled = true;
        else delete next.disabled;
      }
      if (patch.settings !== undefined) {
        if (patch.settings === null || isEmptySettings(patch.settings)) delete next.settings;
        else next.settings = patch.settings;
      }
      if (patch.notes !== undefined) {
        if (patch.notes.trim().length === 0) delete next.notes;
        else next.notes = patch.notes;
      }
      if (patch.pinnedInput !== undefined) {
        if (patch.pinnedInput === null) delete next.pinnedInput;
        else next.pinnedInput = patch.pinnedInput;
      }
      return next;
    }),
  });
}

export function applyAction(
  graph: ScenarioGraphDto,
  action: GraphAction,
  catalogue: Catalogue,
): GraphResult {
  switch (action.type) {
    case "addNode":
      if (findNode(graph, action.node.id) !== undefined) return ok(graph);
      return ok({ ...graph, nodes: [...graph.nodes, action.node] }, [action.node.id]);
    case "moveNodes":
      return ok({
        ...graph,
        nodes: graph.nodes.map((node) => {
          const position = action.positions[node.id];
          return position === undefined ? node : { ...node, position };
        }),
      });
    case "remove":
      return ok(removeNodes(removeLinks(graph, new Set(action.linkIds)), new Set(action.nodeIds)));
    case "connect": {
      const reason = connectRefusal(graph, catalogue, action.from, action.to);
      if (reason !== null) return { ok: false, refusal: { kind: "connect", reason } };
      return ok({ ...graph, edges: [...graph.edges, mainEdge(action.from, action.to)] });
    }
    case "reroute":
      return reroute(graph, catalogue, action.linkId, action.from, action.to);
    case "attach":
      return attach(graph, catalogue, action.parentId, action.childId, action.slot);
    case "duplicate":
      return duplicate(graph, action.nodeIds);
    case "updateNode":
      return updateNode(graph, action.nodeId, action.patch);
    case "replace":
      return ok(action.graph);
  }
}
