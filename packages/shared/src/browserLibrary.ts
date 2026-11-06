import { z } from "zod";

export const HISTORY_TITLE_MAX = 2048;
export const HISTORY_QUERY_MAX = 512;
export const HISTORY_PAGE_MAX = 500;
export const BOOKMARKS_MAX = 5000;
export const BOOKMARK_FOLDER_MAX = 1024;
export const IMPORT_SOURCES_MAX = 64;
export const IMPORT_LABEL_MAX = 256;
export const IMPORT_SOURCE_ID_MAX = 4096;

const id = z.string().uuid();
const url = z.string().url().max(8192);
const timestamp = z.number().int().nonnegative();

export const BROWSER_IMPORT_KINDS = [
  "chrome",
  "edge",
  "brave",
  "vivaldi",
  "yandex",
  "opera",
  "firefox",
] as const;
export const BrowserImportKind = z.enum(BROWSER_IMPORT_KINDS);
export type BrowserImportKind = z.infer<typeof BrowserImportKind>;

export const BrowserHistoryEntryDto = z.object({
  id,
  url,
  title: z.string().max(HISTORY_TITLE_MAX),
  visitedAt: timestamp,
  importedFrom: BrowserImportKind.nullable(),
});
export type BrowserHistoryEntryDto = z.infer<typeof BrowserHistoryEntryDto>;

export const BrowserHistoryDto = z.object({
  items: z.array(BrowserHistoryEntryDto).max(HISTORY_PAGE_MAX),
  total: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
});
export type BrowserHistoryDto = z.infer<typeof BrowserHistoryDto>;

export const BrowserHistoryQuery = z.object({
  query: z.string().max(HISTORY_QUERY_MAX).optional(),
});
export type BrowserHistoryQuery = z.infer<typeof BrowserHistoryQuery>;

export const BrowserLibraryRef = z.object({ id });
export type BrowserLibraryRef = z.infer<typeof BrowserLibraryRef>;

export const BrowserBookmarkDto = z.object({
  id,
  url,
  title: z.string().max(HISTORY_TITLE_MAX),
  folder: z.string().max(BOOKMARK_FOLDER_MAX),
  createdAt: timestamp,
});
export type BrowserBookmarkDto = z.infer<typeof BrowserBookmarkDto>;

export const BrowserBookmarksDto = z.object({
  items: z.array(BrowserBookmarkDto).max(BOOKMARKS_MAX),
});
export type BrowserBookmarksDto = z.infer<typeof BrowserBookmarksDto>;

export const BrowserBookmarkRenameInput = z.object({
  id,
  title: z.string().min(1).max(HISTORY_TITLE_MAX),
});
export type BrowserBookmarkRenameInput = z.infer<typeof BrowserBookmarkRenameInput>;

export const BrowserImportSourceDto = z.object({
  id: z.string().min(1).max(IMPORT_SOURCE_ID_MAX),
  kind: BrowserImportKind,
  profile: z.string().max(IMPORT_LABEL_MAX),
  history: z.boolean(),
  bookmarks: z.boolean(),
});
export type BrowserImportSourceDto = z.infer<typeof BrowserImportSourceDto>;

export const BrowserImportSourcesDto = z.object({
  sources: z.array(BrowserImportSourceDto).max(IMPORT_SOURCES_MAX),
});
export type BrowserImportSourcesDto = z.infer<typeof BrowserImportSourcesDto>;

export const BrowserImportInput = z
  .object({
    source: z.string().min(1).max(IMPORT_SOURCE_ID_MAX),
    history: z.boolean(),
    bookmarks: z.boolean(),
  })
  .refine((input) => input.history || input.bookmarks);
export type BrowserImportInput = z.infer<typeof BrowserImportInput>;

export const BrowserImportResultDto = z.object({
  history: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
  bookmarks: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
});
export type BrowserImportResultDto = z.infer<typeof BrowserImportResultDto>;
