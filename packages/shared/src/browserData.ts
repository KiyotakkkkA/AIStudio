import { z } from "zod";

export const DOWNLOAD_NAME_MAX = 1024;
export const DOWNLOAD_PATH_MAX = 32_767;
export const DOWNLOAD_HISTORY_MAX = 500;

export const BROWSER_DOWNLOAD_STATES = ["done", "failed", "cancelled"] as const;
export const BrowserDownloadState = z.enum(BROWSER_DOWNLOAD_STATES);
export type BrowserDownloadState = z.infer<typeof BrowserDownloadState>;

export const BrowserDownloadDto = z.object({
  id: z.string().uuid(),
  url: z.string().url().max(8192),
  fileName: z.string().min(1).max(DOWNLOAD_NAME_MAX),
  path: z.string().min(1).max(DOWNLOAD_PATH_MAX).nullable(),
  bytes: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
  state: BrowserDownloadState,
  agent: z.boolean(),
  finishedAt: z.number().int().nonnegative(),
});
export type BrowserDownloadDto = z.infer<typeof BrowserDownloadDto>;

export const BrowserDownloadRef = z.object({ id: z.string().uuid() });
export type BrowserDownloadRef = z.infer<typeof BrowserDownloadRef>;

export const BrowserDownloadsDto = z.object({
  items: z.array(BrowserDownloadDto).max(DOWNLOAD_HISTORY_MAX),
});
export type BrowserDownloadsDto = z.infer<typeof BrowserDownloadsDto>;
