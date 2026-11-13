import { z } from "zod";
import { ConversationId, MessageId } from "./primitives/branded.js";
import { Timestamp } from "./primitives/time.js";

export const COMPACT_INSTRUCTIONS_MAX = 1000;
export const COMPACT_SUMMARY_MAX = 60_000;
export const COMPACT_MODEL_ID_MAX = 200;
export const RECAP_MAX = 4000;
export const COMPACTION_STEP_DOMAIN = "compaction";
export const RECAP_STEP_DOMAIN = "recap";

const Uuid = z.uuid();

export const CompactionTrigger = z.enum(["manual", "auto"]);
export type CompactionTrigger = z.infer<typeof CompactionTrigger>;

export const CompactionPhase = z.enum(["started", "finished", "failed"]);
export type CompactionPhase = z.infer<typeof CompactionPhase>;

export const Recap = z
  .string()
  .max(RECAP_MAX)
  .refine((text) => text.trim().length > 0);

export const ChatCompactionDto = z.object({
  id: Uuid,
  boundaryMessageId: MessageId,
  summary: z
    .string()
    .max(COMPACT_SUMMARY_MAX)
    .refine((text) => text.trim().length > 0),
  messages: z.number().int().positive(),
  tokensBefore: z.number().int().nonnegative(),
  tokensAfter: z.number().int().nonnegative(),
  providerId: z.string().min(1).max(COMPACT_MODEL_ID_MAX),
  modelId: z.string().min(1).max(COMPACT_MODEL_ID_MAX),
  trigger: CompactionTrigger,
  createdAt: Timestamp,
});
export type ChatCompactionDto = z.infer<typeof ChatCompactionDto>;

export const ChatCompactInput = z.object({
  conversationId: ConversationId,
  instructions: z.string().max(COMPACT_INSTRUCTIONS_MAX).optional(),
});
export type ChatCompactInput = z.infer<typeof ChatCompactInput>;

export const ChatCompactionRef = z.object({
  conversationId: ConversationId,
  compactionId: Uuid,
});
export type ChatCompactionRef = z.infer<typeof ChatCompactionRef>;

export const ChatCompactionStepDto = z.object({
  domain: z.literal(COMPACTION_STEP_DOMAIN),
  conversationId: ConversationId,
  trigger: CompactionTrigger,
  phase: CompactionPhase,
});
export type ChatCompactionStepDto = z.infer<typeof ChatCompactionStepDto>;

export const ChatRecapStepDto = z.object({
  domain: z.literal(RECAP_STEP_DOMAIN),
  conversationId: ConversationId,
  messageId: MessageId,
  recap: Recap,
});
export type ChatRecapStepDto = z.infer<typeof ChatRecapStepDto>;
