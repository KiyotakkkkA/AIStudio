import {
  mdiBellOutline,
  mdiBrain,
  mdiClockOutline,
  mdiCogOutline,
  mdiDatabaseOutline,
  mdiDownloadOutline,
  mdiEmailOutline,
  mdiFileDocumentOutline,
  mdiFormatListBulleted,
  mdiMessageArrowRightOutline,
  mdiMessageOutline,
  mdiMessagePlusOutline,
  mdiReply,
  mdiRobotOutline,
  mdiSourceBranch,
  mdiTextBoxOutline,
  mdiTimerSandEmpty,
  mdiToolboxOutline,
  mdiVectorPoint,
  mdiWebhook,
  mdiWrenchOutline,
} from "@mdi/js";
import type { Edge, Node } from "@xyflow/react";
import type { ScenarioNodeDto, ScenarioNodeRole, ScenarioSlot } from "@zvs/shared";
import { attachmentLinkId, childrenIn, edgeLinkId } from "./graphModel";
import { nodeSize, slotOffset } from "./layout";
import type ScenarioEditor from "./ScenarioEditor";
import { slotHandle } from "./ScenarioEditor";
import { ROLE_LABELS, slotLabel, TOOL_SET_EMPTY, toolCountLabel } from "./scenarioCopy";
import { isOutputType } from "./nodeForms";
import { TOOL_SET_TYPE, toolSetKeys } from "./toolSet";

export const IN_HANDLE = "in";
export const OUT_HANDLE = "out";
export const UP_HANDLE = "up";

export type DropState = "accept" | "refuse" | null;

export interface SlotPort {
  readonly slot: ScenarioSlot;
  readonly handle: string;
  readonly label: string;
  readonly offset: number;
  readonly filled: boolean;
  readonly drop: DropState;
}

export interface ScenarioNodeData extends Record<string, unknown> {
  readonly label: string;
  readonly subLabel: string;
  readonly role: ScenarioNodeRole | null;
  readonly icon: string;
  readonly width: number;
  readonly height: number;
  readonly hasIn: boolean;
  readonly hasOut: boolean;
  readonly hasUp: boolean;
  readonly slots: readonly SlotPort[];
  readonly chips: readonly string[];
  readonly issues: readonly string[];
  readonly ceiling: string | null;
  readonly disabled: boolean;
}

export interface ScenarioEdgeData extends Record<string, unknown> {
  readonly kind: "main" | "attachment";
}

export type ScenarioFlowNode = Node<ScenarioNodeData, "scenario">;
export type ScenarioFlowEdge = Edge<ScenarioEdgeData, "scenario">;

const TYPE_ICONS: Readonly<Record<string, string>> = {
  "trigger.chat": mdiMessageOutline,
  "trigger.mail": mdiEmailOutline,
  "trigger.schedule": mdiClockOutline,
  "trigger.webhook": mdiWebhook,
  "agent.tools": mdiRobotOutline,
  "model.chat": mdiBrain,
  "memory.window": mdiDatabaseOutline,
  "embedding.model": mdiVectorPoint,
  "tool.set": mdiToolboxOutline,
  "output.chatReply": mdiReply,
  "output.chatMessage": mdiMessageArrowRightOutline,
  "output.newChat": mdiMessagePlusOutline,
  "output.notify": mdiBellOutline,
  "output.file": mdiFileDocumentOutline,
  "llm.generate": mdiTextBoxOutline,
  "flow.branch": mdiSourceBranch,
  "flow.map": mdiFormatListBulleted,
  "job.run": mdiTimerSandEmpty,
  "download.fetch": mdiDownloadOutline,
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
  return typeof modelId === "string" && modelId.length > 0 ? `${base} · ${modelId}` : base;
}

export function flowNodes(editor: ScenarioEditor): ScenarioFlowNode[] {
  const graph = editor.graph;
  const selected = new Set(editor.selection.nodes);
  const drop = editor.dropTarget;
  return graph.nodes.map((node) => {
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
        hasOut:
          (role === "action" || role === "trigger" || role === null) && !isOutputType(node.type),
        hasUp: (spec?.fills.length ?? 0) > 0,
        slots,
        chips,
        issues: editor.issueLines(node.id),
        ceiling: editor.ceilingOf(node.id),
        disabled: node.disabled === true,
      },
    };
  });
}

export function flowEdges(editor: ScenarioEditor): ScenarioFlowEdge[] {
  const graph = editor.graph;
  const selected = new Set(editor.selection.links);
  const main: ScenarioFlowEdge[] = graph.edges.map((edge) => {
    const id = edgeLinkId(edge);
    return {
      id,
      type: "scenario",
      source: edge.from,
      target: edge.to,
      sourceHandle: OUT_HANDLE,
      targetHandle: IN_HANDLE,
      selected: selected.has(id),
      reconnectable: true,
      data: { kind: "main" },
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
      data: { kind: "attachment" },
    };
  });
  return [...main, ...attachments];
}
