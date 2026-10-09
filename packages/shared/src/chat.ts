import { z } from "zod";
import {
  ConversationId,
  MessageId,
  ProviderId,
  RunId,
  VectorStoreId,
} from "./primitives/branded.js";
import { Json } from "./primitives/json.js";
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
});
export type ChatToolCallDto = z.infer<typeof ChatToolCallDto>;
export const ChatEnabledTools = z.array(ToolKey).max(64);
export const CreateConversationInput = z.object({
  title: z.string().trim().min(1).max(200).default("New conversation"),
  providerId: ProviderId,
  modelId: z.string().min(1),
  settings: ChatSettings.default({}),
  systemPrompt: z.string().max(100000).default(""),
  attachedStoreIds: z.array(VectorStoreId).max(64).default([]),
  useRelations: z.boolean().default(false),
  enabledTools: ChatEnabledTools.default([]),
});
export type CreateConversationInput = z.infer<typeof CreateConversationInput>;
export const ConversationRef = z.object({ id: ConversationId });
export const ConversationDto = CreateConversationInput.extend({
  id: ConversationId,
  createdAt: z.number().int(),
  updatedAt: z.number().int(),
});
export type ConversationDto = z.infer<typeof ConversationDto>;
export const MessageDto = z.object({
  id: MessageId,
  conversationId: ConversationId,
  role: z.enum(["user", "assistant"]),
  content: z.string(),
  reasoning: z.string().default(""),
  citations: z.array(ChatCitationDto),
  toolCalls: z.array(ChatToolCallDto).default([]),
  tokensIn: z.number().int().nonnegative(),
  tokensOut: z.number().int().nonnegative(),
  usageEstimated: z.boolean(),
  durationMs: z.number().int().nonnegative(),
  runId: RunId.nullable(),
  partial: z.boolean(),
  createdAt: z.number().int(),
});
export type MessageDto = z.infer<typeof MessageDto>;
export const ConversationDetailDto = ConversationDto.extend({ messages: z.array(MessageDto) });
export type ConversationDetailDto = z.infer<typeof ConversationDetailDto>;
export const ChatSendInput = z.object({
  conversationId: ConversationId,
  text: z.string().trim().min(1).max(100000),
  providerId: ProviderId.optional(),
  modelId: z.string().min(1).optional(),
  settings: ChatSettings.optional(),
});
export type ChatSendInput = z.infer<typeof ChatSendInput>;
export const TruncateConversationInput = ConversationRef.extend({ messageId: MessageId });
export type TruncateConversationInput = z.infer<typeof TruncateConversationInput>;
