import { z } from "zod";
import { DownloadId } from "../primitives/branded.js";
import { RunId } from "../primitives/branded.js";
import { Timestamp } from "../primitives/time.js";

/** What a catalogue entry is. The download lands the artefact; wiring it up belongs elsewhere. */
export const DownloadItemKind = z.enum(["model", "embedding", "runtime", "mcp", "skill"]);
export type DownloadItemKind = z.infer<typeof DownloadItemKind>;

export const DownloadStatus = z.enum([
  "queued",
  "running",
  "paused",
  "succeeded",
  "failed",
  "cancelled",
]);
export type DownloadStatus = z.infer<typeof DownloadStatus>;

export const TERMINAL_DOWNLOAD_STATUSES: readonly DownloadStatus[] = [
  "succeeded",
  "failed",
  "cancelled",
];

export const ChecksumAlgorithm = z.enum(["sha256", "blake3"]);
export type ChecksumAlgorithm = z.infer<typeof ChecksumAlgorithm>;

export const ChecksumDto = z.object({
  algorithm: ChecksumAlgorithm,
  value: z
    .string()
    .trim()
    .regex(/^[0-9a-fA-F]{32,128}$/),
});
export type ChecksumDto = z.infer<typeof ChecksumDto>;

/**
 * A stable handle for a catalogue entry, independent of the source it came from:
 * `<source>:<kind>:<name>`, e.g. `curated:model:qwen3-coder:480b`.
 */
export const ItemRef = z.string().trim().min(3).max(256);
export type ItemRef = z.infer<typeof ItemRef>;

export const CatalogueSource = z.enum(["curated", "ollama", "github"]);
export type CatalogueSource = z.infer<typeof CatalogueSource>;

/**
 * What the catalogue row shows. `available` and `blockedReason` carry the mockup's
 * "not enough free disk" case, which is a precondition rather than an afterthought.
 */
export const CatalogueItemState = z.enum([
  "installed",
  "update",
  "available",
  "downloading",
  "queued",
]);
export type CatalogueItemState = z.infer<typeof CatalogueItemState>;

export const CatalogueItemDto = z.object({
  ref: ItemRef,
  kind: DownloadItemKind,
  source: CatalogueSource,
  name: z.string().min(1).max(200),
  displayName: z.string().min(1).max(200),
  description: z.string().max(1000).default(""),
  version: z.string().max(64).optional(),
  installedVersion: z.string().max(64).optional(),
  sizeBytes: z.number().int().nonnegative(),
  url: z.url().max(2048),
  checksum: ChecksumDto.optional(),
  tags: z.array(z.string().max(48)).max(16).default([]),
  dimension: z.number().int().positive().optional(),
  state: CatalogueItemState,
  downloadable: z.boolean(),
  blockedReason: z.string().max(400).optional(),
  downloadId: DownloadId.optional(),
});
export type CatalogueItemDto = z.infer<typeof CatalogueItemDto>;

export const DownloadDto = z.object({
  id: DownloadId,
  itemKind: DownloadItemKind,
  itemRef: ItemRef,
  displayName: z.string().min(1).max(200),
  sizeBytes: z.number().int().nonnegative(),
  bytesDone: z.number().int().nonnegative(),
  status: DownloadStatus,
  priority: z.number().int().min(0).max(9),
  targetPath: z.string().min(1).max(4096),
  url: z.url().max(2048),
  checksum: ChecksumDto.optional(),
  version: z.string().max(64).optional(),
  error: z.string().max(2000).optional(),
  runId: RunId.optional(),
  /** Smoothed bytes per second; zero unless the transfer is moving right now. */
  rateBytesPerSecond: z.number().nonnegative().default(0),
  etaMs: z.number().int().nonnegative().optional(),
  createdAt: Timestamp,
  updatedAt: Timestamp,
  startedAt: Timestamp.optional(),
  finishedAt: Timestamp.optional(),
});
export type DownloadDto = z.infer<typeof DownloadDto>;

export const DiskCategory = z.enum([
  "models",
  "embeddings",
  "runtimes",
  "mcp",
  "skills",
  "vectors",
  "other",
]);
export type DiskCategory = z.infer<typeof DiskCategory>;

export const DiskUsageDto = z.object({
  root: z.string().min(1),
  totalBytes: z.number().int().nonnegative(),
  freeBytes: z.number().int().nonnegative(),
  usedBytes: z.number().int().nonnegative(),
  categories: z.array(z.object({ category: DiskCategory, bytes: z.number().int().nonnegative() })),
  measuredAt: Timestamp,
});
export type DiskUsageDto = z.infer<typeof DiskUsageDto>;

export const DownloadRef = z.object({ id: DownloadId });
export type DownloadRef = z.infer<typeof DownloadRef>;

export const PrioritiseDownloadInput = z.object({
  id: DownloadId,
  priority: z.number().int().min(0).max(9),
});
export type PrioritiseDownloadInput = z.infer<typeof PrioritiseDownloadInput>;

export const StartDownloadInput = z.object({
  ref: ItemRef,
  priority: z.number().int().min(0).max(9).optional(),
});
export type StartDownloadInput = z.infer<typeof StartDownloadInput>;

export const DownloadListFilter = z.object({
  statuses: z.array(DownloadStatus).max(DownloadStatus.options.length).optional(),
  kinds: z.array(DownloadItemKind).max(DownloadItemKind.options.length).optional(),
});
export type DownloadListFilter = z.infer<typeof DownloadListFilter>;

export const CatalogueFilter = z.object({
  kinds: z.array(DownloadItemKind).max(DownloadItemKind.options.length).optional(),
  states: z.array(CatalogueItemState).max(CatalogueItemState.options.length).optional(),
  query: z.string().trim().max(200).optional(),
  /**
   * Re-query the live sources instead of answering from the shipped catalogue and its cache.
   * The shipped catalogue never reaches the network; only this does.
   */
  refresh: z.boolean().default(false),
});
export type CatalogueFilter = z.infer<typeof CatalogueFilter>;

export const DownloadFetchInput = z.object({ downloadId: DownloadId });
export type DownloadFetchInput = z.infer<typeof DownloadFetchInput>;

export const DownloadReportDto = z.object({
  id: DownloadId,
  bytes: z.number().int().nonnegative(),
  resumedFrom: z.number().int().nonnegative(),
  path: z.string().min(1),
  checksum: z.string().optional(),
});
export type DownloadReportDto = z.infer<typeof DownloadReportDto>;
