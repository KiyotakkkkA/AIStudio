import { z } from "zod";
import { RunId, ScenarioId, ScenarioVersionId } from "./primitives/branded.js";
import { GRAPH_TIMEOUT_MAX_MS } from "./runs/RunDto.js";
import { Timestamp } from "./primitives/time.js";

export const SCENARIO_NAME_MAX = 120;
export const SCENARIO_TAG_MAX = 32;
export const SCENARIO_TAGS_MAX = 16;
export const SCENARIO_NOTE_MAX = 500;
export const SCENARIO_NODE_ID_MAX = 64;
export const SCENARIO_NODE_TYPE_MAX = 128;
export const SCENARIO_NODE_NAME_MAX = 120;
export const SCENARIO_PORT_MAX = 64;
export const SCENARIO_PARAM_KEY_MAX = 128;
export const SCENARIO_NODES_MAX = 200;
export const SCENARIO_EDGES_MAX = 1000;
export const SCENARIO_ATTACHMENTS_MAX = 1000;
export const SCENARIO_ISSUES_MAX = 1000;
export const SCENARIO_MAIN_PORT = "main";
export const SCENARIO_NOTES_MAX = 2000;
export const SCENARIO_ATTEMPTS_MAX = 10;
export const SCENARIO_BACKOFF_MAX_MS = 60_000;

const trimmedName = (max: number) =>
  z
    .string()
    .refine((value) => value.trim().length > 0)
    .refine((value) => value.trim().length <= max);

const NodeId = z.string().min(1).max(SCENARIO_NODE_ID_MAX);
const Port = z.string().min(1).max(SCENARIO_PORT_MAX);
const Tags = z.array(trimmedName(SCENARIO_TAG_MAX)).max(SCENARIO_TAGS_MAX);
const Note = z.string().max(SCENARIO_NOTE_MAX);
const Version = z.number().int().min(1).max(Number.MAX_SAFE_INTEGER);
const Count = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER);

export const ScenarioParams = z.record(z.string().min(1).max(SCENARIO_PARAM_KEY_MAX), z.json());
export type ScenarioParams = z.infer<typeof ScenarioParams>;

export const ScenarioSlot = z.enum(["model", "memory", "tool", "embedding"]);
export type ScenarioSlot = z.infer<typeof ScenarioSlot>;

export const ScenarioTriggerKind = z.enum(["chat", "mail", "schedule", "webhook"]);
export type ScenarioTriggerKind = z.infer<typeof ScenarioTriggerKind>;

export const ScenarioOnError = z.enum(["stop", "continue"]);
export type ScenarioOnError = z.infer<typeof ScenarioOnError>;

export const ScenarioOverlap = z.enum(["skip", "queue"]);
export type ScenarioOverlap = z.infer<typeof ScenarioOverlap>;

export const ScenarioNodeRole = z.enum(["trigger", "action", "subNode"]);
export type ScenarioNodeRole = z.infer<typeof ScenarioNodeRole>;

export const ScenarioIssueRule = z.enum([
  "duplicateNode",
  "unknownNodeType",
  "unknownNode",
  "duplicateEdge",
  "invalidEdge",
  "cycle",
  "unsupportedSlot",
  "slotMismatch",
  "multipleParents",
  "missingAttachment",
  "tooManyAttachments",
  "missingParameter",
  "noTrigger",
  "multipleTriggers",
  "unreachable",
]);
export type ScenarioIssueRule = z.infer<typeof ScenarioIssueRule>;

export const ScenarioPositionDto = z.object({ x: z.number(), y: z.number() });
export type ScenarioPositionDto = z.infer<typeof ScenarioPositionDto>;

export const ScenarioNodeSettingsDto = z.object({
  maxAttempts: z.number().int().min(1).max(SCENARIO_ATTEMPTS_MAX).optional(),
  backoffMs: z.number().int().min(0).max(SCENARIO_BACKOFF_MAX_MS).optional(),
  timeoutMs: z.number().int().min(1).max(GRAPH_TIMEOUT_MAX_MS).optional(),
  onError: ScenarioOnError.optional(),
});
export type ScenarioNodeSettingsDto = z.infer<typeof ScenarioNodeSettingsDto>;

export const ScenarioNodeDto = z.object({
  id: NodeId,
  type: z.string().min(1).max(SCENARIO_NODE_TYPE_MAX),
  name: z.string().max(SCENARIO_NODE_NAME_MAX).optional(),
  position: ScenarioPositionDto,
  params: ScenarioParams,
  disabled: z.boolean().optional(),
  settings: ScenarioNodeSettingsDto.optional(),
  notes: z.string().max(SCENARIO_NOTES_MAX).optional(),
  pinnedInput: z.json().optional(),
});
export type ScenarioNodeDto = z.infer<typeof ScenarioNodeDto>;

export const ScenarioEdgeDto = z.object({
  from: NodeId,
  to: NodeId,
  fromPort: Port,
  toPort: Port,
});
export type ScenarioEdgeDto = z.infer<typeof ScenarioEdgeDto>;

export const ScenarioAttachmentDto = z.object({
  parentNodeId: NodeId,
  childNodeId: NodeId,
  slot: ScenarioSlot,
});
export type ScenarioAttachmentDto = z.infer<typeof ScenarioAttachmentDto>;

export const ScenarioGraphDto = z.object({
  nodes: z.array(ScenarioNodeDto).max(SCENARIO_NODES_MAX),
  edges: z.array(ScenarioEdgeDto).max(SCENARIO_EDGES_MAX),
  attachments: z.array(ScenarioAttachmentDto).max(SCENARIO_ATTACHMENTS_MAX),
});
export type ScenarioGraphDto = z.infer<typeof ScenarioGraphDto>;

export const ScenarioIssueDto = z.object({
  rule: ScenarioIssueRule,
  nodeId: NodeId.optional(),
  slot: ScenarioSlot.optional(),
  param: z.string().min(1).max(SCENARIO_PARAM_KEY_MAX).optional(),
});
export type ScenarioIssueDto = z.infer<typeof ScenarioIssueDto>;

const Issues = z.array(ScenarioIssueDto).max(SCENARIO_ISSUES_MAX);

export const ScenarioTriggerDto = z.object({
  kind: ScenarioTriggerKind,
  nodeId: NodeId,
  params: ScenarioParams,
});
export type ScenarioTriggerDto = z.infer<typeof ScenarioTriggerDto>;

const NodeIds = z.array(NodeId).max(SCENARIO_NODES_MAX);

export const ScenarioDiffDto = z.object({
  nodesAdded: NodeIds,
  nodesRemoved: NodeIds,
  nodesChanged: NodeIds,
  edgesAdded: Count,
  edgesRemoved: Count,
  attachmentsAdded: Count,
  attachmentsRemoved: Count,
});
export type ScenarioDiffDto = z.infer<typeof ScenarioDiffDto>;

export const ScenarioDto = z.object({
  id: ScenarioId,
  name: trimmedName(SCENARIO_NAME_MAX),
  tags: Tags,
  active: z.boolean(),
  overlap: ScenarioOverlap,
  currentVersionId: ScenarioVersionId,
  version: Version,
  trigger: ScenarioTriggerDto.nullable(),
  issueCount: Count,
  createdAt: Timestamp,
  updatedAt: Timestamp,
});
export type ScenarioDto = z.infer<typeof ScenarioDto>;

export const ScenarioDetailDto = ScenarioDto.extend({
  graph: ScenarioGraphDto,
  issues: Issues,
});
export type ScenarioDetailDto = z.infer<typeof ScenarioDetailDto>;

export const ScenarioVersionDto = z.object({
  id: ScenarioVersionId,
  scenarioId: ScenarioId,
  version: Version,
  note: Note.nullable(),
  summary: ScenarioDiffDto,
  createdAt: Timestamp,
});
export type ScenarioVersionDto = z.infer<typeof ScenarioVersionDto>;

export const ScenarioRef = z.object({ id: ScenarioId });
export type ScenarioRef = z.infer<typeof ScenarioRef>;

export const CreateScenarioInput = z.object({
  name: trimmedName(SCENARIO_NAME_MAX),
  tags: Tags.optional(),
  graph: ScenarioGraphDto.optional(),
  note: Note.optional(),
});
export type CreateScenarioInput = z.infer<typeof CreateScenarioInput>;

export const UpdateScenarioInput = z.object({
  id: ScenarioId,
  name: trimmedName(SCENARIO_NAME_MAX).optional(),
  tags: Tags.optional(),
  graph: ScenarioGraphDto.optional(),
  note: Note.optional(),
  overlap: ScenarioOverlap.optional(),
});
export type UpdateScenarioInput = z.infer<typeof UpdateScenarioInput>;

export const ScenarioValidateInput = z.object({ graph: ScenarioGraphDto });
export type ScenarioValidateInput = z.infer<typeof ScenarioValidateInput>;

export const ScenarioValidationDto = z
  .object({ valid: z.boolean(), issues: Issues })
  .refine((result) => result.valid === (result.issues.length === 0), { path: ["valid"] });
export type ScenarioValidationDto = z.infer<typeof ScenarioValidationDto>;

export const SetScenarioActiveInput = z.object({ id: ScenarioId, active: z.boolean() });
export type SetScenarioActiveInput = z.infer<typeof SetScenarioActiveInput>;

export const ScenarioRemoveOutput = z.object({ id: ScenarioId, removed: z.literal(true) });
export type ScenarioRemoveOutput = z.infer<typeof ScenarioRemoveOutput>;

export const ScenarioSlotSpecDto = z.object({
  slot: ScenarioSlot,
  required: z.boolean(),
  multiple: z.boolean(),
});
export type ScenarioSlotSpecDto = z.infer<typeof ScenarioSlotSpecDto>;

export const ScenarioNodeTypeDto = z.object({
  type: z.string().min(1).max(SCENARIO_NODE_TYPE_MAX),
  role: ScenarioNodeRole,
  trigger: ScenarioTriggerKind.nullable(),
  fills: z.array(ScenarioSlot),
  slots: z.array(ScenarioSlotSpecDto),
  requiredParams: z.array(z.string().min(1).max(SCENARIO_PARAM_KEY_MAX)),
  toolKey: z.string().min(1).max(SCENARIO_NODE_TYPE_MAX).nullable(),
  paramsSchema: z.json(),
  idempotent: z.boolean(),
});
export type ScenarioNodeTypeDto = z.infer<typeof ScenarioNodeTypeDto>;

export const ScenarioDraftDto = z.object({
  scenarioId: ScenarioId,
  graph: ScenarioGraphDto,
  savedAt: Timestamp,
});
export type ScenarioDraftDto = z.infer<typeof ScenarioDraftDto>;

export const SaveScenarioDraftInput = z.object({ id: ScenarioId, graph: ScenarioGraphDto });
export type SaveScenarioDraftInput = z.infer<typeof SaveScenarioDraftInput>;

export const ScenarioRunInput = z.object({
  id: ScenarioId,
  graph: ScenarioGraphDto.optional(),
  input: z.json().optional(),
});
export type ScenarioRunInput = z.infer<typeof ScenarioRunInput>;

export const ScenarioRunNodeInput = z.object({
  id: ScenarioId,
  graph: ScenarioGraphDto,
  nodeId: NodeId,
  input: z.json().optional(),
});
export type ScenarioRunNodeInput = z.infer<typeof ScenarioRunNodeInput>;

export const ScenarioRetryStepInput = z.object({ runId: RunId, nodeId: NodeId });
export type ScenarioRetryStepInput = z.infer<typeof ScenarioRetryStepInput>;
