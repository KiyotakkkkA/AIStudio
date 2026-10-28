import {
  mdiBellOutline,
  mdiBrain,
  mdiClockOutline,
  mdiCogOutline,
  mdiDatabaseSearchOutline,
  mdiEmailOutline,
  mdiFileDocumentOutline,
  mdiFormatListBulleted,
  mdiHistory,
  mdiLightningBoltOutline,
  mdiMessageArrowRightOutline,
  mdiMessageOutline,
  mdiMessagePlusOutline,
  mdiNotebookOutline,
  mdiReply,
  mdiRobotOutline,
  mdiSourceBranch,
  mdiToolboxOutline,
  mdiVectorPoint,
  mdiWebhook,
  mdiWrenchOutline,
} from "@mdi/js";
import type { Edge, Node } from "@xyflow/react";
import {
  SCENARIO_MAIN_PORT,
  type ScenarioNodeDto,
  type ScenarioNodeRole,
  type ScenarioSlot,
} from "@zvs/shared";
import { attachmentLinkId, childrenIn, edgeLinkId } from "./graphModel";
import { nodeSize, slotOffset } from "./layout";
import type ScenarioEditor from "./ScenarioEditor";
import { slotHandle } from "./ScenarioEditor";
import {
  issueText,
  PORT_LABELS,
  ROLE_LABELS,
  slotLabel,
  STORES_EMPTY,
  storesWord,
  TOOL_SET_EMPTY,
  toolCountLabel,
} from "./scenarioCopy";
import { isWarning, STORES_TYPE, storeIds } from "./nodeRules";
import type { PaletteAnchor } from "./palette";
import type { NodeRunStatus } from "./runModel";
import { TOOL_SET_TYPE, toolSetKeys } from "./toolSet";

export const IN_HANDLE = "in";
export const OUT_HANDLE = "out";
export const UP_HANDLE = "up";
export const PORT_HANDLE_PREFIX = "out:";

export function portHandle(port: string): string {
  return port === SCENARIO_MAIN_PORT ? OUT_HANDLE : `${PORT_HANDLE_PREFIX}${port}`;
}

export function portOfHandle(handle: string | null | undefined): string | null {
  if (handle === OUT_HANDLE) return SCENARIO_MAIN_PORT;
  if (handle?.startsWith(PORT_HANDLE_PREFIX) !== true) return null;
  return handle.slice(PORT_HANDLE_PREFIX.length);
}

export type DropState = "accept" | "refuse" | null;

export interface SlotPort {
  readonly slot: ScenarioSlot;
  readonly handle: string;
  readonly label: string;
  readonly offset: number;
  readonly filled: boolean;
  readonly drop: DropState;
}

export interface OutPort {
  readonly port: string;
  readonly handle: string;
  readonly label: string | null;
  readonly top: number;
}

export interface NextAnchor {
  readonly key: string;
  readonly anchor: PaletteAnchor;
  readonly side: "right" | "bottom";
  readonly offset: number;
}

export interface ScenarioNodeData extends Record<string, unknown> {
  readonly label: string;
  readonly subLabel: string;
  readonly role: ScenarioNodeRole | null;
  readonly icon: string;
  readonly width: number;
  readonly height: number;
  readonly hasIn: boolean;
  readonly outs: readonly OutPort[];
  readonly hasUp: boolean;
  readonly slots: readonly SlotPort[];
  readonly next: readonly NextAnchor[];
  readonly chips: readonly string[];
  readonly issues: readonly string[];
  readonly warnings: readonly string[];
  readonly ceiling: string | null;
  readonly disabled: boolean;
  readonly run: NodeRunStatus | null;
  readonly runError: string | null;
}

export interface ScenarioEdgeData extends Record<string, unknown> {
  readonly kind: "main" | "attachment";
  readonly traversed: boolean;
  readonly items: number | null;
}

export type ScenarioFlowNode = Node<ScenarioNodeData, "scenario">;
export type ScenarioFlowEdge = Edge<ScenarioEdgeData, "scenario">;

const TYPE_ICONS: Readonly<Record<string, string>> = {
  "trigger.chat": mdiMessageOutline,
  "trigger.mail": mdiEmailOutline,
  "trigger.schedule": mdiClockOutline,
  "trigger.webhook": mdiWebhook,
  "trigger.integration": mdiLightningBoltOutline,
  "agent.tools": mdiRobotOutline,
  "model.chat": mdiBrain,
  "memory.window": mdiHistory,
  "memory.facts": mdiNotebookOutline,
  "knowledge.stores": mdiDatabaseSearchOutline,
  "embedding.model": mdiVectorPoint,
  "tool.set": mdiToolboxOutline,
  "output.chatReply": mdiReply,
  "output.chatMessage": mdiMessageArrowRightOutline,
  "output.newChat": mdiMessagePlusOutline,
  "output.notify": mdiBellOutline,
  "output.file": mdiFileDocumentOutline,
  "flow.branch": mdiSourceBranch,
  "flow.map": mdiFormatListBulleted,
};

const SLOT_CHIPS: Record<Exclude<ScenarioSlot, "tool">, string> = {
  model: "модель",
  memory: "память",
  embedding: "эмбеддинги",
};

export function iconFor(type: string, toolKey: string | null): string {
  return TYPE_ICONS[type] ?? (toolKey === null ? mdiCogOutline : mdiWrenchOutline);
}

function chipFor(slot: ScenarioSlot, count: number): string {
  return slot === "tool" ? toolCountLabel(count) : SLOT_CHIPS[slot];
}

function subLabelOf(node: ScenarioNodeDto, base: string, modelId: unknown): string {
  if (node.type === TOOL_SET_TYPE) {
    const count = toolSetKeys(node.params).length;
    return count === 0 ? TOOL_SET_EMPTY : toolCountLabel(count);
  }
  if (node.type === STORES_TYPE) {
    const count = storeIds(node.params).length;
    return count === 0 ? STORES_EMPTY : storesWord(count);
  }
  return typeof modelId === "string" && modelId.length > 0 ? `${base} · ${modelId}` : base;
}

export function flowNodes(editor: ScenarioEditor): ScenarioFlowNode[] {
  const graph = editor.graph;
  const selected = new Set(editor.selection.nodes);
  const drop = editor.dropTarget;
  const runs = editor.execution.nodes;
  return graph.nodes.map((node) => {
    const run = runs.get(node.id);
    const spec = editor.catalogue.get(node.type);
    const size = nodeSize(spec);
    const role = spec?.role ?? null;
    const modelId = node.params.modelId;
    const roleLabel = role === null ? node.type : ROLE_LABELS[role];
    const base = node.name === undefined ? roleLabel : editor.labelOf(node.type);
    const slots: SlotPort[] = (spec?.slots ?? []).map((entry) => ({
      slot: entry.slot,
      handle: slotHandle(entry.slot),
      label: slotLabel(entry.slot, entry.required),
      offset: spec === undefined ? 0 : (slotOffset(spec, entry.slot) ?? 0),
      filled: childrenIn(graph, node.id, entry.slot).length > 0,
      drop:
        drop !== null && drop.parentId === node.id && drop.slot === entry.slot
          ? drop.refusal === null
            ? "accept"
            : "refuse"
          : null,
    }));
    const ports = spec?.ports ?? [SCENARIO_MAIN_PORT];
    const outs: OutPort[] = ports.map((port, index) => ({
      port,
      handle: portHandle(port),
      label: PORT_LABELS[port] ?? null,
      top: Math.round(((index + 1) * size.height) / (ports.length + 1)),
    }));
    const editable = selected.has(node.id) && editor.selection.nodes.length === 1;
    const next: NextAnchor[] = editable
      ? [
          ...outs
            .filter(
              (out) =>
                !graph.edges.some((edge) => edge.from === node.id && edge.fromPort === out.port),
            )
            .map((out) => ({
              key: out.handle,
              anchor: { nodeId: node.id, kind: "port" as const, port: out.port },
              side: "right" as const,
              offset: out.top,
            })),
          ...slots
            .filter(
              (port) =>
                !port.filled ||
                spec?.slots.find((entry) => entry.slot === port.slot)?.multiple === true,
            )
            .map((port) => ({
              key: port.handle,
              anchor: { nodeId: node.id, kind: "slot" as const, slot: port.slot },
              side: "bottom" as const,
              offset: port.offset,
            })),
        ]
      : [];
    const issues = editor.issuesByNode.get(node.id) ?? [];
    const chips = (spec?.slots ?? []).flatMap((entry) => {
      const count = childrenIn(graph, node.id, entry.slot).reduce((sum, attachment) => {
        const child = graph.nodes.find((candidate) => candidate.id === attachment.childNodeId);
        return sum + (child?.type === TOOL_SET_TYPE ? toolSetKeys(child.params).length : 1);
      }, 0);
      return count === 0 ? [] : [chipFor(entry.slot, count)];
    });
    return {
      id: node.id,
      type: "scenario",
      position: editor.positionOf(node),
      selected: selected.has(node.id),
      width: size.width,
      height: size.height,
      data: {
        label: editor.nameOf(node.id),
        subLabel: subLabelOf(node, base, modelId),
        role,
        icon: iconFor(node.type, spec?.toolKey ?? null),
        width: size.width,
        height: size.height,
        hasIn: role === "action" || role === null,
        outs,
        hasUp: (spec?.fills.length ?? 0) > 0,
        slots,
        next,
        chips,
        issues: issues
          .filter((issue) => !isWarning(issue))
          .map((issue) => issueText(issue, editor.nameOf)),
        warnings: issues.filter(isWarning).map((issue) => issueText(issue, editor.nameOf)),
        ceiling: editor.ceilingOf(node.id),
        disabled: node.disabled === true,
        run: run?.status ?? null,
        runError: run?.error ?? null,
      },
    };
  });
}

export function flowEdges(editor: ScenarioEditor): ScenarioFlowEdge[] {
  const graph = editor.graph;
  const selected = new Set(editor.selection.links);
  const runs = editor.execution.edges(graph);
  const main: ScenarioFlowEdge[] = graph.edges.map((edge) => {
    const id = edgeLinkId(edge);
    const run = runs.get(id);
    return {
      id,
      type: "scenario",
      source: edge.from,
      target: edge.to,
      sourceHandle: portHandle(edge.fromPort),
      targetHandle: IN_HANDLE,
      selected: selected.has(id),
      reconnectable: true,
      data: { kind: "main", traversed: run?.traversed ?? false, items: run?.items ?? null },
    };
  });
  const attachments: ScenarioFlowEdge[] = graph.attachments.map((attachment) => {
    const id = attachmentLinkId(attachment);
    return {
      id,
      type: "scenario",
      source: attachment.parentNodeId,
      target: attachment.childNodeId,
      sourceHandle: slotHandle(attachment.slot),
      targetHandle: UP_HANDLE,
      selected: selected.has(id),
      reconnectable: false,
      data: {
        kind: "attachment",
        traversed: editor.execution.nodes.get(attachment.parentNodeId)?.status === "succeeded",
        items: null,
      },
    };
  });
  return [...main, ...attachments];
}
