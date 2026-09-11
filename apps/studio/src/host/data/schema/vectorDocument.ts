import { integer, sqliteTable, text, uniqueIndex } from "drizzle-orm/sqlite-core";
import { vectorStore } from "./vectorStore.ts";

export const vectorDocument = sqliteTable(
  "vector_document",
  {
    id: text("id").primaryKey(),
    storeId: text("store_id")
      .notNull()
      .references(() => vectorStore.id, { onDelete: "cascade" }),
    sourcePath: text("source_path").notNull(),
    contentHash: text("content_hash").notNull(),
    chunkCount: integer("chunk_count").notNull(),
    bytes: integer("bytes").notNull(),
    indexedAt: integer("indexed_at").notNull(),
  },
  (table) => [uniqueIndex("vector_document_store_path_unq").on(table.storeId, table.sourcePath)],
);

export type VectorDocumentEntity = typeof vectorDocument.$inferSelect;
export type VectorDocumentInsert = typeof vectorDocument.$inferInsert;
