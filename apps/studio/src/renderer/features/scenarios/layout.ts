import type {
  ScenarioGraphDto,
  ScenarioNodeDto,
  ScenarioNodeTypeDto,
  ScenarioPositionDto,
  ScenarioSlot,
} from "@zvs/shared";
import { attachRefusal, findNode, isTrunk, parentOf, type Catalogue } from "./graphModel";

export interface NodeSize {
  readonly width: number;
  readonly height: number;
}

export const TRUNK_SIZE: NodeSize = { width: 188, height: 62 };
export const PARENT_SIZE: NodeSize = { width: 212, height: 82 };
export const SUB_SIZE: NodeSize = { width: 144, height: 58 };
export const SNAP_RADIUS = 44;

const COLUMN = 300;
const ROW_GAP = 64;
const SUB_GAP_Y = 96;
const SUB_GAP_X = 24;
const ORIGIN = 40;

export function nodeSize(spec: ScenarioNodeTypeDto | undefined): NodeSize {
  if (spec === undefined) return TRUNK_SIZE;
  if (spec.slots.length > 0) return PARENT_SIZE;
  return spec.role === "subNode" ? SUB_SIZE : TRUNK_SIZE;
}

export function slotOffset(spec: ScenarioNodeTypeDto, slot: ScenarioSlot): number | null {
  const index = spec.slots.findIndex((entry) => entry.slot === slot);
  if (index < 0) return null;
  const width = nodeSize(spec).width;
  return Math.round(((index + 1) * width) / (spec.slots.length + 1));
}

export interface SlotTarget {
  readonly parentId: string;
  readonly slot: ScenarioSlot;
  readonly refusal: ReturnType<typeof attachRefusal>;
}

export function slotUnder(
  graph: ScenarioGraphDto,
  catalogue: Catalogue,
  childId: string,
  position: ScenarioPositionDto,
): SlotTarget | null {
  const child = findNode(graph, childId);
  if (child === undefined) return null;
  const childSize = nodeSize(catalogue.get(child.type));
  const anchor = { x: position.x + childSize.width / 2, y: position.y };
  let best: { target: SlotTarget; distance: number } | null = null;
  for (const parent of graph.nodes) {
    if (parent.id === childId) continue;
    const spec = catalogue.get(parent.type);
    if (spec === undefined || spec.slots.length === 0) continue;
    const size = nodeSize(spec);
    for (const entry of spec.slots) {
      const offset = slotOffset(spec, entry.slot);
      if (offset === null) continue;
      const distance = Math.hypot(
        parent.position.x + offset - anchor.x,
        parent.position.y + size.height - anchor.y,
      );
      if (distance > SNAP_RADIUS || (best !== null && distance >= best.distance)) continue;
      best = {
        distance,
        target: {
          parentId: parent.id,
          slot: entry.slot,
          refusal: attachRefusal(graph, catalogue, parent.id, childId, entry.slot),
        },
      };
    }
  }
  return best?.target ?? null;
}

function layers(graph: ScenarioGraphDto, trunk: readonly ScenarioNodeDto[]): Map<string, number> {
  const ids = new Set(trunk.map((node) => node.id));
  const edges = graph.edges.filter((edge) => ids.has(edge.from) && ids.has(edge.to));
  const incoming = new Map<string, number>(trunk.map((node) => [node.id, 0]));
  for (const edge of edges) incoming.set(edge.to, (incoming.get(edge.to) ?? 0) + 1);
  const layer = new Map<string, number>();
  const queue = trunk.filter((node) => incoming.get(node.id) === 0).map((node) => node.id);
  for (const id of queue) layer.set(id, 0);
  while (queue.length > 0) {
    const id = queue.shift();
    if (id === undefined) break;
    for (const edge of edges) {
      if (edge.from !== id) continue;
      layer.set(edge.to, Math.max(layer.get(edge.to) ?? 0, (layer.get(id) ?? 0) + 1));
      const remaining = (incoming.get(edge.to) ?? 0) - 1;
      incoming.set(edge.to, remaining);
      if (remaining === 0) queue.push(edge.to);
    }
  }
  for (const node of trunk) if (!layer.has(node.id)) layer.set(node.id, 0);
  return layer;
}

function childrenOf(graph: ScenarioGraphDto, catalogue: Catalogue, parent: ScenarioNodeDto) {
  const order = catalogue.get(parent.type)?.slots.map((entry) => entry.slot) ?? [];
  return graph.attachments
    .filter((attachment) => attachment.parentNodeId === parent.id)
    .sort((left, right) => order.indexOf(left.slot) - order.indexOf(right.slot))
    .map((attachment) => findNode(graph, attachment.childNodeId))
    .filter((node): node is ScenarioNodeDto => node !== undefined);
}

function subtreeDepth(
  graph: ScenarioGraphDto,
  catalogue: Catalogue,
  node: ScenarioNodeDto,
  seen: Set<string> = new Set(),
): number {
  if (seen.has(node.id)) return 0;
  seen.add(node.id);
  const children = childrenOf(graph, catalogue, node);
  if (children.length === 0) return 0;
  return 1 + Math.max(...children.map((child) => subtreeDepth(graph, catalogue, child, seen)));
}

function placeChildren(
  graph: ScenarioGraphDto,
  catalogue: Catalogue,
  parent: ScenarioNodeDto,
  at: ScenarioPositionDto,
  positions: Map<string, ScenarioPositionDto>,
): void {
  const children = childrenOf(graph, catalogue, parent).filter((child) => !positions.has(child.id));
  if (children.length === 0) return;
  const parentSize = nodeSize(catalogue.get(parent.type));
  const sizes = children.map((child) => nodeSize(catalogue.get(child.type)));
  const total =
    sizes.reduce((sum, size) => sum + size.width, 0) + SUB_GAP_X * (children.length - 1);
  let x = at.x + parentSize.width / 2 - total / 2;
  const y = at.y + parentSize.height + SUB_GAP_Y;
  children.forEach((child, index) => {
    const position = { x: Math.round(x), y: Math.round(y) };
    positions.set(child.id, position);
    x += (sizes[index]?.width ?? 0) + SUB_GAP_X;
    placeChildren(graph, catalogue, child, position, positions);
  });
}

export function tidy(graph: ScenarioGraphDto, catalogue: Catalogue): ScenarioGraphDto {
  const trunk = graph.nodes.filter(
    (node) => isTrunk(catalogue, node) && parentOf(graph, node.id) === undefined,
  );
  const layer = layers(graph, trunk);
  const columns = new Map<number, ScenarioNodeDto[]>();
  for (const node of trunk) {
    const index = layer.get(node.id) ?? 0;
    columns.set(index, [...(columns.get(index) ?? []), node]);
  }
  const positions = new Map<string, ScenarioPositionDto>();
  for (const [index, column] of columns) {
    let y = ORIGIN;
    for (const node of [...column].sort((left, right) => left.position.y - right.position.y)) {
      const position = { x: ORIGIN + index * COLUMN, y };
      positions.set(node.id, position);
      placeChildren(graph, catalogue, node, position, positions);
      const size = nodeSize(catalogue.get(node.type));
      y +=
        size.height +
        subtreeDepth(graph, catalogue, node) * (SUB_SIZE.height + SUB_GAP_Y) +
        ROW_GAP;
    }
  }
  const bottom = Math.max(
    ORIGIN,
    ...[...positions.entries()].map(([id, position]) => {
      const node = findNode(graph, id);
      return (
        position.y + nodeSize(node === undefined ? undefined : catalogue.get(node.type)).height
      );
    }),
  );
  let x = ORIGIN;
  for (const node of graph.nodes) {
    if (positions.has(node.id)) continue;
    const position = { x, y: bottom + ROW_GAP };
    positions.set(node.id, position);
    placeChildren(graph, catalogue, node, position, positions);
    x += nodeSize(catalogue.get(node.type)).width + SUB_GAP_X;
  }
  return {
    ...graph,
    nodes: graph.nodes.map((node) => ({
      ...node,
      position: positions.get(node.id) ?? node.position,
    })),
  };
}
