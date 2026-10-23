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
});
export type ChatToolCallDto = z.infer<typeof ChatToolCallDto>;
export const ChatEnabledTools = z.array(ToolKey).max(64);
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
  enabledTools: ChatEnabledTools.default([]),
  retrievalMode: RetrievalMode.default("hybrid"),
});
export type CreateConversationInput = z.infer<typeof CreateConversationInput>;
export const ConversationRef = z.object({ id: ConversationId });
const ConversationFields = CreateConversationInput.extend({
  id: ConversationId,
  workingFolder: FolderPath.nullable().default(null),
  workingFolderAccess: WorkingFolderAccess.nullable().default(null),
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
    documentId: DocumentId,
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
  .refine((selection) => selection.end > selection.start, { path: ["end"] });
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
  tokensIn: z.number().int().nonnegative(),
  tokensOut: z.number().int().nonnegative(),
  usageEstimated: z.boolean(),
  durationMs: z.number().int().nonnegative(),
  runId: RunId.nullable(),
  partial: z.boolean(),
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
});
export type ChatSendInput = z.infer<typeof ChatSendInput>;
export const ChatContextInput = z.object({
  conversationId: ConversationId.optional(),
  providerId: ProviderId,
  modelId: z.string().min(1),
  settings: ChatSettings.optional(),
  systemPrompt: z.string().max(100000).optional(),
  enabledTools: ChatEnabledTools.optional(),
});
export type ChatContextInput = z.infer<typeof ChatContextInput>;
export const ChatContextToolDto = z.object({
  key: ToolKey,
  tokens: z.number().int().nonnegative(),
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
