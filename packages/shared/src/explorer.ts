import { z } from "zod";
import { ConversationId, StreamId } from "./primitives/branded.js";

export const EXPLORER_STEP_DOMAIN = "workspace";
export const EXPLORER_LIST_MAX = 5000;
export const EXPLORER_TEXT_MAX = 1_048_576;

const RelativePath = z.string().max(32767);
const EntryPath = RelativePath.min(1);

export const ExplorerEntryKind = z.enum(["file", "folder"]);
export type ExplorerEntryKind = z.infer<typeof ExplorerEntryKind>;

export const ExplorerChangeKind = z.enum(["created", "modified", "removed"]);
export type ExplorerChangeKind = z.infer<typeof ExplorerChangeKind>;

export const ExplorerEntryDto = z.object({
  name: z.string().min(1),
  path: EntryPath,
  kind: ExplorerEntryKind,
  size: z.number().int().nonnegative(),
  modifiedAt: z.number().int().nonnegative().nullable(),
  ignored: z.boolean(),
});
export type ExplorerEntryDto = z.infer<typeof ExplorerEntryDto>;

export const ExplorerListInput = z.object({ id: ConversationId, path: RelativePath });
export type ExplorerListInput = z.infer<typeof ExplorerListInput>;

export const ExplorerListingDto = z.object({
  path: RelativePath,
  entries: z.array(ExplorerEntryDto).max(EXPLORER_LIST_MAX),
  truncated: z.boolean(),
});
export type ExplorerListingDto = z.infer<typeof ExplorerListingDto>;

export const ExplorerReadInput = z.object({ id: ConversationId, path: EntryPath });
export type ExplorerReadInput = z.infer<typeof ExplorerReadInput>;

export const ExplorerFileDto = z
  .object({
    path: EntryPath,
    size: z.number().int().nonnegative(),
    modifiedAt: z.number().int().nonnegative().nullable(),
    text: z.string().max(EXPLORER_TEXT_MAX).nullable(),
    truncated: z.boolean(),
  })
  .refine((file) => !file.truncated || file.text !== null, { path: ["truncated"] });
export type ExplorerFileDto = z.infer<typeof ExplorerFileDto>;

export const ExplorerChangeDto = z.object({ path: RelativePath, kind: ExplorerChangeKind });
export type ExplorerChangeDto = z.infer<typeof ExplorerChangeDto>;

export const ExplorerChangesDto = z.object({
  domain: z.literal(EXPLORER_STEP_DOMAIN),
  conversationId: ConversationId,
  changes: z.array(ExplorerChangeDto).min(1).max(500),
});
export type ExplorerChangesDto = z.infer<typeof ExplorerChangesDto>;

export const ExplorerWatchDto = z.object({ streamId: StreamId });
export type ExplorerWatchDto = z.infer<typeof ExplorerWatchDto>;
