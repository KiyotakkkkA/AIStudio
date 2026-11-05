import { z } from "zod";
import { ImageAttachments } from "./images.js";
import {
  ConversationId,
  DocumentId,
  IntegrationId,
  MessageId,
  ProviderId,
  RunId,
  VectorStoreId,
} from "./primitives/branded.js";
import { Json } from "./primitives/json.js";
import { IntegrationKind } from "./integrations.js";
import { FolderGrantScope, FolderPath, WorkingFolderAccess } from "./folders.js";
import { ToolKey } from "./tools/ToolDto.js";
import { WebUrl } from "./sites.js";
import { ToolAuthorisationDto } from "./execPolicy.js";
import { RunHandleDto } from "./runs/RunDto.js";
import { ApprovalRequestDto, EventSeq } from "./events/HostEvent.js";
import { QuestionRequestDto } from "./questions.js";
import { Timestamp } from "./primitives/time.js";
import { AgenticPlanStepDto } from "./agentic.js";
import { EndedShellDto, SHELLS_TOTAL_MAX } from "./shells.js";

export const ChatSettings = z.object({
  temperature: z.number().min(0).max(2).optional(),
  topK: z.number().int().positive().optional(),
  topP: z.number().min(0).max(1).optional(),
  maxOutputTokens: z.number().int().positive().optional(),
});
export type ChatSettings = z.infer<typeof ChatSettings>;
export const ChatCitationDto = z.object({
  storeId: VectorStoreId,
  documentId: z.string(),
  sourcePath: z.string(),
  chunkIndex: z.number().int().nonnegative(),
  score: z.number().min(0).max(1),
  section: z.string().default(""),
  page: z.number().int().positive().nullable().default(null),
  related: z.string().nullable().default(null),
});
export type ChatCitationDto = z.infer<typeof ChatCitationDto>;
export const CHAT_TOOL_CALL_STATUSES = [
  "running",
  "awaiting-approval",
  "awaiting-answer",
  "succeeded",
  "failed",
  "refused",
  "denied",
  "cancelled",
] as const;
export const ChatToolCallStatus = z.enum(CHAT_TOOL_CALL_STATUSES);
export type ChatToolCallStatus = z.infer<typeof ChatToolCallStatus>;
export const ChatRoundDto = z.object({
  text: z.string(),
  reasoning: z.string(),
  tokensIn: z.number().int().nonnegative().optional(),
  tokensOut: z.number().int().nonnegative().optional(),
  estimated: z.boolean().default(false),
});
export type ChatRoundDto = z.infer<typeof ChatRoundDto>;
export const ChatToolCallDto = z.object({
  id: z.string().min(1).max(128),
  key: z.string().min(1).max(200),
  name: z.string().min(1).max(200),
  round: z.number().int().nonnegative(),
  arguments: Json,
  status: ChatToolCallStatus,
  result: z.string().nullable().default(null),
  truncated: z.boolean().default(false),
  error: z.string().nullable().default(null),
  approvalId: z.string().nullable().default(null),
  stepId: z.string().nullable().default(null),
  durationMs: z.number().int().nonnegative().default(0),
  meta: Json.nullable().default(null),
  authorisation: ToolAuthorisationDto.optional(),
});
export type ChatToolCallDto = z.infer<typeof ChatToolCallDto>;
export const OFFERED_TOOL_SOURCES = [
  "base",
  "knowledge",
  "memory",
  "subagent",
  "command",
  "loaded",
] as const;
export const OfferedToolSource = z.enum(OFFERED_TOOL_SOURCES);
export type OfferedToolSource = z.infer<typeof OfferedToolSource>;
export const OfferedToolDto = z.object({
  key: z.string().min(1).max(200),
  source: OfferedToolSource,
});
export type OfferedToolDto = z.infer<typeof OfferedToolDto>;
export const THINKING_EFFORTS = ["auto", "thinking", "fast"] as const;
export const ThinkingEffort = z.enum(THINKING_EFFORTS);
export type ThinkingEffort = z.infer<typeof ThinkingEffort>;
export const VendorTurnOptions = z.object({
  effort: ThinkingEffort.optional(),
  webSearch: z.boolean().default(false),
});
export type VendorTurnOptions = z.infer<typeof VendorTurnOptions>;
export const WEB_SOURCES_MAX = 200;
export const WebSourceDto = z.object({
  url: z.url().max(4096),
  title: z.string().min(1).max(4000),
  snippet: z.string().max(4000).default(""),
  date: z.string().max(100).nullable().default(null),
  icon: WebUrl.optional(),
});
export type WebSourceDto = z.infer<typeof WebSourceDto>;
export const CHAT_COMMANDS = ["browser-use", "computer-use", "goal"] as const;
export const ChatCommand = z.enum(CHAT_COMMANDS);
export type ChatCommand = z.infer<typeof ChatCommand>;
export const CHAT_COMMANDS_MAX = 8;
export const CHAT_PLUGINS_MAX = 16;
function distinct(items: readonly unknown[]): boolean {
  return new Set(items).size === items.length;
}
export const ChatCommands = z
  .array(ChatCommand)
  .max(CHAT_COMMANDS_MAX)
  .refine(distinct, { message: "unique" });
export const ChatPluginIds = z
  .array(IntegrationId)
  .max(CHAT_PLUGINS_MAX)
  .refine(distinct, { message: "unique" });
export const ChatPluginDto = z.object({
  accountId: IntegrationId,
  kind: IntegrationKind,
  name: z.string().trim().min(1).max(120),
});
export type ChatPluginDto = z.infer<typeof ChatPluginDto>;
export const ChatCommandDto = z.object({
  command: ChatCommand,
  description: z.string().trim().min(1).max(500),
});
export type ChatCommandDto = z.infer<typeof ChatCommandDto>;
export const RetrievalMode = z.enum(["auto", "agentic", "hybrid"]);
export type RetrievalMode = z.infer<typeof RetrievalMode>;
export const ChatEvalInput = z.object({
  storeId: VectorStoreId,
  providerId: ProviderId,
  modelId: z.string().min(1),
  modes: z.array(RetrievalMode).min(1).max(3).optional(),
});
export type ChatEvalInput = z.infer<typeof ChatEvalInput>;
export const ChatEvalCaseDto = z.object({
  caseId: z.string(),
  question: z.string(),
  cited: z.array(z.string()),
  hit: z.boolean(),
  searches: z.number().int().nonnegative(),
  error: z.string().nullable(),
});
export type ChatEvalCaseDto = z.infer<typeof ChatEvalCaseDto>;
export const ChatEvalModeDto = z.object({
  mode: RetrievalMode,
  hitRate: z.number().min(0).max(1),
  averageSearches: z.number().nonnegative(),
  cases: z.array(ChatEvalCaseDto),
});
export type ChatEvalModeDto = z.infer<typeof ChatEvalModeDto>;
export const ChatEvalRunDto = z.object({
  storeId: VectorStoreId,
  modelId: z.string(),
  ranAt: z.number().int(),
  modes: z.array(ChatEvalModeDto),
});
export type ChatEvalRunDto = z.infer<typeof ChatEvalRunDto>;
export const CreateConversationInput = z.object({
  title: z.string().trim().min(1).max(200).default("Новый диалог"),
  providerId: ProviderId,
  modelId: z.string().min(1),
  settings: ChatSettings.default({}),
  systemPrompt: z.string().max(100000).default(""),
  attachedStoreIds: z.array(VectorStoreId).max(64).default([]),
  useRelations: z.boolean().default(false),
  retrievalMode: RetrievalMode.default("hybrid"),
});
export type CreateConversationInput = z.infer<typeof CreateConversationInput>;
export const ConversationRef = z.object({ id: ConversationId });

export const EXPORT_PATH_MAX = 32767;

export const ConversationExportFormat = z.enum(["json", "markdown"]);
export type ConversationExportFormat = z.infer<typeof ConversationExportFormat>;

export const ExportConversationInput = ConversationRef.extend({ format: ConversationExportFormat });
export type ExportConversationInput = z.infer<typeof ExportConversationInput>;

export const ConversationExportDto = z.object({
  path: z.string().min(1).max(EXPORT_PATH_MAX).optional(),
});
export type ConversationExportDto = z.infer<typeof ConversationExportDto>;
export const GOAL_STEP_DOMAIN = "goal";
export const GOAL_SUMMARY_MAX = 4000;
export const GoalStatus = z.enum(["active", "paused", "done", "stopped", "failed"]);
export type GoalStatus = z.infer<typeof GoalStatus>;
export const GoalReason = z.enum([
  "rounds",
  "tokens",
  "minutes",
  "repetition",
  "restart",
  "user",
  "error",
]);
export type GoalReason = z.infer<typeof GoalReason>;
const GoalCount = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER);
export const GoalBudgetDto = z.object({
  rounds: GoalCount,
  tokens: GoalCount,
  minutes: GoalCount,
});
export type GoalBudgetDto = z.infer<typeof GoalBudgetDto>;
export const GoalUsageDto = z.object({
  rounds: GoalCount,
  tokens: GoalCount,
  elapsedMs: GoalCount,
});
export type GoalUsageDto = z.infer<typeof GoalUsageDto>;
const REASONED_GOAL: readonly GoalStatus[] = ["paused", "stopped", "failed"];
const FINISHED_GOAL: readonly GoalStatus[] = ["done", "stopped", "failed"];
const reasonMatchesStatus = (goal: { status: GoalStatus; reason: GoalReason | null }) =>
  REASONED_GOAL.includes(goal.status) === (goal.reason !== null);
export const ChatGoalDto = z
  .object({
    text: z
      .string()
      .max(100000)
      .refine((text) => text.trim().length > 0),
    status: GoalStatus,
    reason: GoalReason.nullable(),
    budget: GoalBudgetDto,
    usage: GoalUsageDto,
    summary: z.string().max(GOAL_SUMMARY_MAX).nullable(),
    plan: z.array(AgenticPlanStepDto).max(50).default([]),
    startedAt: z.number().int().nonnegative(),
    endedAt: z.number().int().nonnegative().nullable(),
  })
  .refine(reasonMatchesStatus, { path: ["reason"] })
  .refine((goal) => goal.summary === null || goal.status === "done", { path: ["summary"] })
  .refine((goal) => FINISHED_GOAL.includes(goal.status) === (goal.endedAt !== null), {
    path: ["endedAt"],
  });
export type ChatGoalDto = z.infer<typeof ChatGoalDto>;
export const ChatGoalStepDto = z
  .object({
    domain: z.literal(GOAL_STEP_DOMAIN),
    conversationId: ConversationId,
    status: GoalStatus,
    reason: GoalReason.nullable(),
    round: GoalCount,
    usage: GoalUsageDto,
    budget: GoalBudgetDto,
    plan: z.array(AgenticPlanStepDto).max(50).default([]),
  })
  .refine(reasonMatchesStatus, { path: ["reason"] });
export type ChatGoalStepDto = z.infer<typeof ChatGoalStepDto>;
export const ChatGoalInput = z.object({ conversationId: ConversationId });
export type ChatGoalInput = z.infer<typeof ChatGoalInput>;
const ConversationFields = CreateConversationInput.extend({
  id: ConversationId,
  workingFolder: FolderPath.nullable().default(null),
  workingFolderAccess: WorkingFolderAccess.nullable().default(null),
  pinned: z.boolean().default(false),
  goal: ChatGoalDto.nullable().default(null),
  createdAt: z.number().int(),
  updatedAt: z.number().int(),
});
const folderAccessMatches = (conversation: {
  workingFolder: string | null;
  workingFolderAccess: WorkingFolderAccess | null;
}) => (conversation.workingFolder === null) === (conversation.workingFolderAccess === null);
export const ConversationDto = ConversationFields.refine(folderAccessMatches, {
  path: ["workingFolderAccess"],
});
export type ConversationDto = z.infer<typeof ConversationDto>;
export const CHAT_SELECTIONS_MAX = 8;
export const SELECTION_TEXT_MAX = 8000;

export const DocumentSelectionDto = z
  .object({
    documentId: DocumentId.nullable(),
    messageId: MessageId.optional(),
    name: z.string().min(1).max(1024),
    page: z.number().int().positive().nullable().default(null),
    start: z.number().int().nonnegative(),
    end: z.number().int().nonnegative(),
    text: z
      .string()
      .max(SELECTION_TEXT_MAX)
      .refine((text) => text.trim().length > 0),
    untrusted: z.literal(true),
  })
  .refine((selection) => selection.end > selection.start, { path: ["end"] })
  .refine((selection) => (selection.documentId === null) !== (selection.messageId === undefined), {
    path: ["messageId"],
  });
export type DocumentSelectionDto = z.infer<typeof DocumentSelectionDto>;

const Selections = z.array(DocumentSelectionDto).max(CHAT_SELECTIONS_MAX);

export const MessageRole = z.enum(["user", "assistant", "event"]);
export type MessageRole = z.infer<typeof MessageRole>;

export const ChatEventDto = z.object({
  shells: z.array(EndedShellDto).min(1).max(SHELLS_TOTAL_MAX),
});
export type ChatEventDto = z.infer<typeof ChatEventDto>;

export const MessageDto = z
  .object({
    id: MessageId,
    conversationId: ConversationId,
    role: MessageRole,
    content: z.string(),
    reasoning: z.string().default(""),
    citations: z.array(ChatCitationDto),
    toolCalls: z.array(ChatToolCallDto).default([]),
    images: ImageAttachments.optional(),
    selections: Selections.default([]),
    commands: ChatCommands.default([]),
    plugins: z.array(ChatPluginDto).max(CHAT_PLUGINS_MAX).default([]),
    offeredTools: z.array(OfferedToolDto).default([]),
    webSources: z.array(WebSourceDto).max(WEB_SOURCES_MAX).default([]),
    rounds: z.array(ChatRoundDto).default([]),
    tokensIn: z.number().int().nonnegative(),
    tokensOut: z.number().int().nonnegative(),
    tokensCached: z.number().int().nonnegative().default(0),
    tokensReasoning: z.number().int().nonnegative().default(0),
    usageEstimated: z.boolean(),
    durationMs: z.number().int().nonnegative(),
    runId: RunId.nullable(),
    partial: z.boolean(),
    pinned: z.boolean().default(false),
    event: ChatEventDto.optional(),
    createdAt: z.number().int(),
  })
  .refine((message) => (message.role === "event") === (message.event !== undefined), {
    path: ["event"],
  });
export type MessageDto = z.infer<typeof MessageDto>;

export const WAKE_STEP_DOMAIN = "wake";

export const ChatWakeStepDto = z.object({
  domain: z.literal(WAKE_STEP_DOMAIN),
  message: MessageDto.refine((message) => message.role === "event", { path: ["role"] }),
});
export type ChatWakeStepDto = z.infer<typeof ChatWakeStepDto>;
export const ConversationDetailDto = ConversationFields.extend({
  messages: z.array(MessageDto),
}).refine(folderAccessMatches, { path: ["workingFolderAccess"] });
export type ConversationDetailDto = z.infer<typeof ConversationDetailDto>;
export const ChatSendInput = z
  .object({
    images: ImageAttachments.default([]),
    conversationId: ConversationId,
    text: z.string().trim().max(100000),
    providerId: ProviderId.optional(),
    modelId: z.string().min(1).optional(),
    settings: ChatSettings.optional(),
    selections: Selections.default([]),
    commands: ChatCommands.default([]),
    plugins: ChatPluginIds.default([]),
    vendor: VendorTurnOptions.optional(),
  })
  .refine((input) => input.text.length > 0 || input.images.length > 0, { path: ["text"] });
export type ChatSendInput = z.infer<typeof ChatSendInput>;

export const QUEUE_STEP_DOMAIN = "queue";
export const CHAT_QUEUE_MAX = 10;
const QueuedMessageId = z.string().min(1).max(64);
const QueuedText = z.string().trim().min(1).max(100000);
export const ChatQueuedMessageDto = z.object({
  id: QueuedMessageId,
  text: QueuedText,
  createdAt: Timestamp,
});
export type ChatQueuedMessageDto = z.infer<typeof ChatQueuedMessageDto>;
export const ChatQueueDto = z.object({
  conversationId: ConversationId,
  messages: z.array(ChatQueuedMessageDto).max(CHAT_QUEUE_MAX),
  urgent: z.boolean(),
});
export type ChatQueueDto = z.infer<typeof ChatQueueDto>;
export const ChatQueueStepDto = ChatQueueDto.extend({ domain: z.literal(QUEUE_STEP_DOMAIN) });
export type ChatQueueStepDto = z.infer<typeof ChatQueueStepDto>;
export const ChatQueueInput = z.object({ conversationId: ConversationId });
export type ChatQueueInput = z.infer<typeof ChatQueueInput>;
export const ChatQueueAddInput = ChatQueueInput.extend({ text: QueuedText });
export type ChatQueueAddInput = z.infer<typeof ChatQueueAddInput>;
export const ChatQueueEditInput = ChatQueueInput.extend({
  messageId: QueuedMessageId,
  text: QueuedText,
});
export type ChatQueueEditInput = z.infer<typeof ChatQueueEditInput>;
export const ChatQueueRemoveInput = ChatQueueInput.extend({ messageId: QueuedMessageId });
export type ChatQueueRemoveInput = z.infer<typeof ChatQueueRemoveInput>;

export const ChatContextInput = z.object({
  conversationId: ConversationId.optional(),
  providerId: ProviderId,
  modelId: z.string().min(1),
  settings: ChatSettings.optional(),
  systemPrompt: z.string().max(100000).optional(),
  commands: ChatCommands.default([]),
  plugins: ChatPluginIds.default([]),
});
export type ChatContextInput = z.infer<typeof ChatContextInput>;
export const ChatContextToolDto = z.object({
  key: ToolKey,
  tokens: z.number().int().nonnegative(),
  source: OfferedToolSource,
});
export type ChatContextToolDto = z.infer<typeof ChatContextToolDto>;
export const ChatContextDto = z.object({
  contextWindow: z.number().int().positive(),
  reservedOutput: z.number().int().nonnegative(),
  system: z.number().int().nonnegative(),
  tools: z.number().int().nonnegative(),
  toolItems: z.array(ChatContextToolDto),
  history: z.number().int().nonnegative(),
  historyMessages: z.number().int().nonnegative(),
  trimmed: z.number().int().nonnegative(),
  total: z.number().int().nonnegative(),
});
export type ChatContextDto = z.infer<typeof ChatContextDto>;
export const CHAT_PROMPT_SECTIONS = [
  "dialogPrompt",
  "profile",
  "memory",
  "knowledge",
  "mcpGuidance",
  "command",
  "skills",
  "workspace",
  "fileTools",
  "processTools",
  "endedShells",
  "ask",
  "delegate",
  "deferred",
  "goal",
  "toolSpecs",
] as const;
export const ChatPromptSection = z.enum(CHAT_PROMPT_SECTIONS);
export type ChatPromptSection = z.infer<typeof ChatPromptSection>;
export const ChatHistoryStatus = z.enum(["included", "trimmed", "skipped"]);
export type ChatHistoryStatus = z.infer<typeof ChatHistoryStatus>;
export const ChatPromptSectionDto = z.object({
  key: ChatPromptSection,
  tokens: z.number().int().nonnegative(),
  text: z.string(),
});
export type ChatPromptSectionDto = z.infer<typeof ChatPromptSectionDto>;
export const ChatCarriedKind = z.enum(["toolCall", "source"]);
export type ChatCarriedKind = z.infer<typeof ChatCarriedKind>;
export const ChatCarriedStatus = z.enum(["sent", "shortened", "dropped"]);
export type ChatCarriedStatus = z.infer<typeof ChatCarriedStatus>;
export const ChatCarriedPartDto = z.object({
  kind: ChatCarriedKind,
  label: z.string().min(1),
  marker: z.string().default(""),
  status: ChatCarriedStatus,
  tokens: z.number().int().nonnegative(),
  truncated: z.boolean().default(false),
});
export type ChatCarriedPartDto = z.infer<typeof ChatCarriedPartDto>;
export const ChatHistoryEntryDto = z.object({
  messageId: MessageId,
  role: MessageRole,
  status: ChatHistoryStatus,
  tokens: z.number().int().nonnegative(),
  text: z.string(),
  reasoningTokens: z.number().int().nonnegative(),
  carried: z.array(ChatCarriedPartDto).default([]),
});
export type ChatHistoryEntryDto = z.infer<typeof ChatHistoryEntryDto>;
export const ChatStatsDto = z.object({
  context: ChatContextDto,
  tokenizer: z.string().min(1),
  sections: z.array(ChatPromptSectionDto),
  deferred: z.array(ToolKey),
  history: z.array(ChatHistoryEntryDto),
});
export type ChatStatsDto = z.infer<typeof ChatStatsDto>;
export const TruncateConversationInput = ConversationRef.extend({ messageId: MessageId });
export type TruncateConversationInput = z.infer<typeof TruncateConversationInput>;
export const BranchConversationInput = ConversationRef.extend({ messageId: MessageId });
export type BranchConversationInput = z.infer<typeof BranchConversationInput>;
export const PinConversationInput = ConversationRef.extend({ pinned: z.boolean() });
export type PinConversationInput = z.infer<typeof PinConversationInput>;
export const PinMessageInput = TruncateConversationInput.extend({ pinned: z.boolean() });
export type PinMessageInput = z.infer<typeof PinMessageInput>;

export const BindFolderInput = z.object({
  id: ConversationId,
  path: FolderPath,
  grant: FolderGrantScope.optional(),
});
export type BindFolderInput = z.infer<typeof BindFolderInput>;
export const BindFolderResultDto = z.discriminatedUnion("status", [
  z.object({ status: z.literal("bound"), conversation: ConversationDto }),
  z.object({ status: z.literal("needsGrant"), path: FolderPath }),
]);
export type BindFolderResultDto = z.infer<typeof BindFolderResultDto>;
export const PickFolderResultDto = z.object({ path: FolderPath.nullable() });
export type PickFolderResultDto = z.infer<typeof PickFolderResultDto>;
export const ChatAttachInput = z.object({ conversationId: ConversationId });
export type ChatAttachInput = z.infer<typeof ChatAttachInput>;
export const ChatAttachDto = z.object({
  run: RunHandleDto,
  seq: EventSeq.nullable(),
  startedAt: Timestamp,
  content: z.string(),
  reasoning: z.string(),
  rounds: z.array(ChatRoundDto).default([]),
  toolCalls: z.array(ChatToolCallDto),
  approvals: z.array(ApprovalRequestDto),
  questions: z.array(QuestionRequestDto),
});
export type ChatAttachDto = z.infer<typeof ChatAttachDto>;
