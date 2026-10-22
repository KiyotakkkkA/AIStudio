import { z } from "zod";
import { ConversationId, RunId } from "./primitives/branded.js";

export const FILES_APPROVAL_KIND = "files";

const FilePath = z.string().min(1).max(32767);

export const FileChangeKind = z.enum(["create", "modify", "move", "delete"]);
export type FileChangeKind = z.infer<typeof FileChangeKind>;

export const FileDiffDto = z
  .object({
    path: FilePath,
    from: FilePath.optional(),
    kind: FileChangeKind,
    added: z.number().int().nonnegative(),
    removed: z.number().int().nonnegative(),
    diff: z.string().max(20000),
    truncated: z.boolean(),
  })
  .refine((change) => (change.kind === "move") === (change.from !== undefined), {
    path: ["from"],
  });
export type FileDiffDto = z.infer<typeof FileDiffDto>;

export const FileChangesDto = z.object({ changes: z.array(FileDiffDto).max(32) });
export type FileChangesDto = z.infer<typeof FileChangesDto>;

export const FilesApprovalDto = z.object({
  id: z.uuid(),
  runId: RunId,
  kind: z.literal(FILES_APPROVAL_KIND),
  subject: z.string().min(1),
  scope: z.string(),
  expiresAt: z.number().int(),
  conversationId: ConversationId,
  tool: z.string().min(1),
  action: z.string().max(2000),
  changes: z.array(FileDiffDto).min(1).max(32),
});
export type FilesApprovalDto = z.infer<typeof FilesApprovalDto>;
