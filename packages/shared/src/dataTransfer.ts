import { z } from "zod";
import { Timestamp } from "./primitives/time.js";

export const TransferCategory = z.enum([
  "conversations",
  "stores",
  "scenarios",
  "skills",
  "memory",
  "connections",
  "settings",
]);
export type TransferCategory = z.infer<typeof TransferCategory>;

export const TransferSelection = z.object({
  categories: z
    .array(TransferCategory)
    .min(1)
    .max(7)
    .refine((items) => new Set(items).size === items.length),
});
export type TransferSelection = z.infer<typeof TransferSelection>;

export const TransferCategoryDto = z.object({
  category: TransferCategory,
  count: z.number().int().nonnegative(),
  bytes: z.number().int().nonnegative(),
  names: z.array(z.string().max(120)).max(20).optional(),
  requires: z.array(TransferCategory).max(7).optional(),
});
export type TransferCategoryDto = z.infer<typeof TransferCategoryDto>;

export const TransferPreviewDto = z.object({
  createdAt: Timestamp,
  categories: z.array(TransferCategoryDto).max(7),
  warnings: z.array(z.enum(["credentials", "indexes", "inactive", "settings", "missingFiles"])),
  blockedCategories: z.array(TransferCategory).max(7).optional(),
  importedCategories: z.array(TransferCategory).max(7).optional(),
});
export type TransferPreviewDto = z.infer<typeof TransferPreviewDto>;

export const TransferUploadInput = z.object({
  name: z
    .string()
    .min(1)
    .max(255)
    .regex(/\.dbox$/i),
  bytes: z
    .number()
    .int()
    .positive()
    .max(8 * 1024 ** 3),
});
export type TransferUploadInput = z.infer<typeof TransferUploadInput>;

export const TransferRef = z.object({ id: z.uuid() });
export type TransferRef = z.infer<typeof TransferRef>;

export const TransferChunkInput = TransferRef.extend({
  offset: z
    .number()
    .int()
    .nonnegative()
    .max(8 * 1024 ** 3),
  data: z
    .string()
    .min(4)
    .max(699052)
    .regex(/^[A-Za-z0-9+/]*={0,2}$/),
});
export type TransferChunkInput = z.infer<typeof TransferChunkInput>;

export const TransferImportInput = TransferRef.extend(TransferSelection.shape);
export type TransferImportInput = z.infer<typeof TransferImportInput>;

export const TransferExportDto = z.object({ path: z.string().min(1) });
export type TransferExportDto = z.infer<typeof TransferExportDto>;

export const TransferImportDto = z.object({
  categories: z.array(TransferCategoryDto),
});
export type TransferImportDto = z.infer<typeof TransferImportDto>;
