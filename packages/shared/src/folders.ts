import { z } from "zod";
import { ConversationId, FolderGrantId, RunId } from "./primitives/branded.js";

export const FOLDER_APPROVAL_KIND = "folder";

export const FolderPath = z.string().min(1).max(32767);

export const FolderGrantScope = z.enum(["always", "conversation"]);
export type FolderGrantScope = z.infer<typeof FolderGrantScope>;

export const WorkingFolderAccess = z.enum(["granted", "missing"]);
export type WorkingFolderAccess = z.infer<typeof WorkingFolderAccess>;

export const FolderAccessAnswer = z.enum(["once", "conversation", "always", "deny"]);
export type FolderAccessAnswer = z.infer<typeof FolderAccessAnswer>;

export const FolderGrantChatDto = z.object({ id: ConversationId, title: z.string() });
export type FolderGrantChatDto = z.infer<typeof FolderGrantChatDto>;

export const FolderGrantDto = z
  .object({
    id: FolderGrantId,
    path: FolderPath,
    scope: FolderGrantScope,
    conversation: FolderGrantChatDto.nullable(),
    boundConversations: z.array(FolderGrantChatDto),
    grantedAt: z.number().int(),
    lastUsedAt: z.number().int().nullable(),
  })
  .refine((grant) => (grant.scope === "conversation") === (grant.conversation !== null), {
    path: ["conversation"],
  });
export type FolderGrantDto = z.infer<typeof FolderGrantDto>;

export const FolderGrantRef = z.object({ id: FolderGrantId });
export type FolderGrantRef = z.infer<typeof FolderGrantRef>;

export const FolderRequestEntryDto = z.object({
  path: FolderPath,
  creates: z.array(FolderPath),
});
export type FolderRequestEntryDto = z.infer<typeof FolderRequestEntryDto>;

export const FolderApprovalDto = z.object({
  id: z.uuid(),
  runId: RunId,
  kind: z.literal(FOLDER_APPROVAL_KIND),
  subject: z.string().min(1),
  scope: z.string(),
  expiresAt: z.number().int(),
  conversationId: ConversationId,
  tool: z.string().min(1),
  action: z.string().max(2000),
  folders: z.array(FolderRequestEntryDto).min(1).max(64),
});
export type FolderApprovalDto = z.infer<typeof FolderApprovalDto>;

export const AnswerFolderRequestInput = z.object({
  id: RunId,
  approvalId: z.uuid(),
  answer: FolderAccessAnswer,
});
export type AnswerFolderRequestInput = z.infer<typeof AnswerFolderRequestInput>;
