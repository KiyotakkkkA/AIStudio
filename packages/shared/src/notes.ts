import { z } from "zod";
import { ConversationId, NoteId } from "./primitives/branded.js";

export const NOTES_FOLDER = "notes";
export const NOTES_ROOT = "chat:notes/";
export const NOTE_EXTENSION = ".md";
export const NOTE_SCHEMA_VERSION = 1;
export const NOTE_NAME_MAX = 120;
export const NOTE_TITLE_MAX = 200;
export const NOTE_CONTEXT_MAX = 2000;
export const NOTE_READ_WHEN_MAX = 600;
export const NOTE_CONDITION_MAX = 2000;
export const NOTE_FILE_MAX_BYTES = 128 * 1024;
export const NOTE_TIME_MAX = 40;
export const NOTE_ERROR_MAX = 500;

const FORBIDDEN = /[<>"|?*:/\\\p{Cc}]/u;

export function isNoteName(name: string): boolean {
  if (!name.toLowerCase().endsWith(NOTE_EXTENSION)) return false;
  const stem = name.slice(0, -NOTE_EXTENSION.length);
  return (
    stem.trim().length > 0 &&
    name.length <= NOTE_NAME_MAX &&
    !name.startsWith(".") &&
    !name.startsWith(" ") &&
    !stem.endsWith(".") &&
    !stem.endsWith(" ") &&
    !FORBIDDEN.test(name)
  );
}

export function notePath(name: string): string {
  return `${NOTES_ROOT}${name}`;
}

const NoteName = z.string().refine(isNoteName);
const NotePath = z
  .string()
  .refine((path) => path.startsWith(NOTES_ROOT) && isNoteName(path.slice(NOTES_ROOT.length)));
const NoteHash = z.string().regex(/^[0-9a-f]{64}$/);
const NoteTime = z.string().min(1).max(NOTE_TIME_MAX);
const NoteError = z.string().min(1).max(NOTE_ERROR_MAX);

function trimmed(max: number) {
  return z
    .string()
    .refine((value) => value.trim().length > 0)
    .refine((value) => value.trim().length <= max);
}

export const NoteSourceDto = z.object({
  id: NoteId,
  title: trimmed(NOTE_TITLE_MAX),
  path: NotePath,
  hash: NoteHash,
});
export type NoteSourceDto = z.infer<typeof NoteSourceDto>;

export const NoteMetaDto = z.object({
  id: NoteId,
  title: trimmed(NOTE_TITLE_MAX),
  createdAt: NoteTime,
  updatedAt: NoteTime,
  context: trimmed(NOTE_CONTEXT_MAX),
  readWhen: trimmed(NOTE_READ_WHEN_MAX),
  updateWhen: trimmed(NOTE_CONDITION_MAX),
  deleteWhen: trimmed(NOTE_CONDITION_MAX),
});
export type NoteMetaDto = z.infer<typeof NoteMetaDto>;

export const NoteSummaryDto = z
  .object({
    name: NoteName,
    path: NotePath,
    size: z.number().int().nonnegative(),
    modifiedAt: z.number().int(),
    id: NoteId.nullable(),
    title: trimmed(NOTE_TITLE_MAX).nullable(),
    updatedAt: NoteTime.nullable(),
    error: NoteError.nullable(),
  })
  .refine((note) => (note.error === null) === (note.id !== null), { path: ["error"] });
export type NoteSummaryDto = z.infer<typeof NoteSummaryDto>;

export const NoteDto = z
  .object({
    name: NoteName,
    path: NotePath,
    hash: NoteHash,
    content: z.string(),
    modifiedAt: z.number().int(),
    meta: NoteMetaDto.nullable(),
    body: z.string().nullable(),
    error: NoteError.nullable(),
  })
  .refine(
    (note) =>
      (note.meta === null) === (note.body === null) &&
      (note.meta === null) !== (note.error === null),
    { path: ["error"] },
  );
export type NoteDto = z.infer<typeof NoteDto>;

export const NoteRef = z.object({ conversationId: ConversationId, name: NoteName });
export type NoteRef = z.infer<typeof NoteRef>;

export const NoteReadInput = z
  .object({
    conversationId: ConversationId,
    name: NoteName.optional(),
    id: NoteId.optional(),
  })
  .refine((input) => (input.name === undefined) !== (input.id === undefined), {
    path: ["name"],
  });
export type NoteReadInput = z.infer<typeof NoteReadInput>;

export const NoteSaveInput = z.object({
  conversationId: ConversationId,
  name: NoteName,
  content: z.string().max(NOTE_FILE_MAX_BYTES),
  expectedHash: NoteHash,
});
export type NoteSaveInput = z.infer<typeof NoteSaveInput>;

export const NoteRenameInput = z.object({
  conversationId: ConversationId,
  name: NoteName,
  to: NoteName,
});
export type NoteRenameInput = z.infer<typeof NoteRenameInput>;
