import { z } from "zod";
import {
  ConversationId,
  DocumentId,
  MessageId,
  ProviderId,
  RunId,
  VectorStoreId,
} from "./primitives/branded.js";
import { Json } from "./primitives/json.js";
import { FolderGrantScope, FolderPath, WorkingFolderAccess } from "./folders.js";
import { ToolKey } from "./tools/ToolDto.js";
import { WebUrl } from "./sites.js";
import { ToolAuthorisationDto } from "./execPolicy.js";
import { RunHandleDto } from "./runs/RunDto.js";
import { ApprovalRequestDto, EventSeq } from "./events/HostEvent.js";
import { QuestionRequestDto } from "./questions.js";
import { Timestamp } from "./primitives/time.js";

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
export const OFFERED_TOOL_SOURCES = ["base", "knowledge", "memory", "command", "loaded"] as const;
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
export const CHAT_COMMANDS = ["browser-use"] as const;
export const ChatCommand = z.enum(CHAT_COMMANDS);
export type ChatCommand = z.infer<typeof ChatCommand>;
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
const ConversationFields = CreateConversationInput.extend({
  id: ConversationId,
  workingFolder: FolderPath.nullable().default(null),
  workingFolderAccess: WorkingFolderAccess.nullable().default(null),
  pinned: z.boolean().default(false),
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

export const MessageDto = z.object({
  id: MessageId,
  conversationId: ConversationId,
  role: z.enum(["user", "assistant"]),
  content: z.string(),
  reasoning: z.string().default(""),
  citations: z.array(ChatCitationDto),
  toolCalls: z.array(ChatToolCallDto).default([]),
  selections: Selections.default([]),
  command: ChatCommand.optional(),
  offeredTools: z.array(OfferedToolDto).default([]),
  webSources: z.array(WebSourceDto).max(WEB_SOURCES_MAX).default([]),
  tokensIn: z.number().int().nonnegative(),
  tokensOut: z.number().int().nonnegative(),
  usageEstimated: z.boolean(),
  durationMs: z.number().int().nonnegative(),
  runId: RunId.nullable(),
  partial: z.boolean(),
  pinned: z.boolean().default(false),
  createdAt: z.number().int(),
});
export type MessageDto = z.infer<typeof MessageDto>;
export const ConversationDetailDto = ConversationFields.extend({
  messages: z.array(MessageDto),
}).refine(folderAccessMatches, { path: ["workingFolderAccess"] });
export type ConversationDetailDto = z.infer<typeof ConversationDetailDto>;
export const ChatSendInput = z.object({
  conversationId: ConversationId,
  text: z.string().trim().min(1).max(100000),
  providerId: ProviderId.optional(),
  modelId: z.string().min(1).optional(),
  settings: ChatSettings.optional(),
  selections: Selections.default([]),
  command: ChatCommand.optional(),
  vendor: VendorTurnOptions.optional(),
});
export type ChatSendInput = z.infer<typeof ChatSendInput>;
export const ChatContextInput = z.object({
  conversationId: ConversationId.optional(),
  providerId: ProviderId,
  modelId: z.string().min(1),
  settings: ChatSettings.optional(),
  systemPrompt: z.string().max(100000).optional(),
  command: ChatCommand.optional(),
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
  toolCalls: z.array(ChatToolCallDto),
  approvals: z.array(ApprovalRequestDto),
  questions: z.array(QuestionRequestDto),
});
export type ChatAttachDto = z.infer<typeof ChatAttachDto>;
