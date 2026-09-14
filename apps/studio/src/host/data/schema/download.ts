import { index, integer, sqliteTable, text } from "drizzle-orm/sqlite-core";
import type { ChecksumAlgorithm, DownloadItemKind, DownloadStatus } from "@zvs/shared";

export const download = sqliteTable(
  "download",
  {
    id: text("id").primaryKey(),
    itemKind: text("item_kind").$type<DownloadItemKind>().notNull(),
    itemRef: text("item_ref").notNull(),
    displayName: text("display_name").notNull(),
    version: text("version"),
    sizeBytes: integer("size_bytes").notNull().default(0),
    bytesDone: integer("bytes_done").notNull().default(0),
    status: text("status").$type<DownloadStatus>().notNull(),
    priority: integer("priority").notNull().default(5),
    targetPath: text("target_path").notNull(),
    url: text("url").notNull(),
    checksumAlgorithm: text("checksum_algorithm").$type<ChecksumAlgorithm>(),
    checksumValue: text("checksum_value"),
    error: text("error"),
    runId: text("run_id"),
    createdAt: integer("created_at").notNull(),
    updatedAt: integer("updated_at").notNull(),
    startedAt: integer("started_at"),
    finishedAt: integer("finished_at"),
  },
  (table) => [
    index("download_status_idx").on(table.status),
    index("download_item_ref_idx").on(table.itemRef),
  ],
);

export type DownloadEntity = typeof download.$inferSelect;
export type DownloadInsert = typeof download.$inferInsert;
