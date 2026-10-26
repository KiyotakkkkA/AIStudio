import { z } from "zod";
import { Timestamp } from "./primitives/time.js";

export const StorageCategory = z.enum([
  "conversations",
  "dashboards",
  "skills",
  "vectors",
  "downloads",
  "toolchain",
  "browser",
  "cache",
  "logs",
  "backups",
  "database",
]);
export type StorageCategory = z.infer<typeof StorageCategory>;

export const StoragePart = z.enum([
  "models",
  "embeddings",
  "runtimes",
  "mcp",
  "skills",
  "node",
  "npm",
  "uv",
  "python",
  "python-packages",
  "other",
]);
export type StoragePart = z.infer<typeof StoragePart>;

export const StoragePartUsage = z.object({
  part: StoragePart,
  bytes: z.number().int().nonnegative(),
});
export type StoragePartUsage = z.infer<typeof StoragePartUsage>;

export const StorageCategoryUsage = z
  .object({
    category: StorageCategory,
    bytes: z.number().int().nonnegative(),
    parts: z.array(StoragePartUsage).optional(),
  })
  .refine(
    (usage) =>
      usage.parts === undefined ||
      usage.parts.length === 0 ||
      usage.parts.reduce((sum, item) => sum + item.bytes, 0) === usage.bytes,
    { path: ["bytes"] },
  );
export type StorageCategoryUsage = z.infer<typeof StorageCategoryUsage>;

export const StorageUsageDto = z
  .object({
    totalBytes: z.number().int().nonnegative(),
    categories: z.array(StorageCategoryUsage),
    measuredAt: Timestamp,
  })
  .refine(
    (usage) => usage.categories.reduce((sum, item) => sum + item.bytes, 0) === usage.totalBytes,
    { path: ["totalBytes"] },
  );
export type StorageUsageDto = z.infer<typeof StorageUsageDto>;

export const StorageUsageInput = z.object({ refresh: z.boolean().default(false) });
export type StorageUsageInput = z.infer<typeof StorageUsageInput>;

export const StorageClearCacheOutput = z.object({
  removedBytes: z.number().int().nonnegative(),
});
export type StorageClearCacheOutput = z.infer<typeof StorageClearCacheOutput>;
