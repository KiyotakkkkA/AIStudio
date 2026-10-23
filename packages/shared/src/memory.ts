import { z } from "zod";
import { ConversationId, MemoryId } from "./primitives/branded.js";

export const MEMORY_CONTENT_MAX = 500;
export const MEMORY_ACCOUNT_MAX = 100;
export const MEMORY_CONVERSATION_MAX = 50;

const MemoryContent = z
  .string()
  .refine((value) => value.trim().length > 0)
  .refine((value) => value.trim().length <= MEMORY_CONTENT_MAX);

export const MemoryScope = z.enum(["account", "conversation"]);
export type MemoryScope = z.infer<typeof MemoryScope>;

export const MemorySource = z.enum(["user", "model"]);
export type MemorySource = z.infer<typeof MemorySource>;

export const MemoryChatDto = z.object({ id: ConversationId, title: z.string() });
export type MemoryChatDto = z.infer<typeof MemoryChatDto>;

export const MemoryDto = z
  .object({
    id: MemoryId,
    scope: MemoryScope,
    conversation: MemoryChatDto.nullable(),
    content: MemoryContent,
    source: MemorySource,
    createdAt: z.number().int(),
    updatedAt: z.number().int(),
  })
  .refine((memory) => (memory.scope === "conversation") === (memory.conversation !== null), {
    path: ["conversation"],
  });
export type MemoryDto = z.infer<typeof MemoryDto>;

const scoped = (input: { scope: MemoryScope; conversationId?: ConversationId | undefined }) =>
  (input.scope === "conversation") === (input.conversationId !== undefined);

export const CreateMemoryInput = z
  .object({
    scope: MemoryScope,
    conversationId: ConversationId.optional(),
    content: MemoryContent,
  })
  .refine(scoped, { path: ["conversationId"] });
export type CreateMemoryInput = z.infer<typeof CreateMemoryInput>;

export const UpdateMemoryInput = z.object({ id: MemoryId, content: MemoryContent });
export type UpdateMemoryInput = z.infer<typeof UpdateMemoryInput>;

export const MemoryRef = z.object({ id: MemoryId });
export type MemoryRef = z.infer<typeof MemoryRef>;

export const ClearMemoryInput = z
  .object({
    scope: MemoryScope,
    conversationId: ConversationId.optional(),
  })
  .refine(scoped, { path: ["conversationId"] });
export type ClearMemoryInput = z.infer<typeof ClearMemoryInput>;
