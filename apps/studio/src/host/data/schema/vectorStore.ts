import { integer, sqliteTable, text, uniqueIndex } from "drizzle-orm/sqlite-core";
import type { VectorStoreDto } from "@zvs/shared";
import { provider } from "./provider.ts";

export const vectorStore = sqliteTable(
  "vector_store",
  {
    id: text("id").primaryKey(),
    name: text("name").notNull(),
    description: text("description").notNull().default(""),
    backend: text("backend").$type<VectorStoreDto["backend"]>().notNull().default("lancedb"),
    embeddingProviderId: text("embedding_provider_id")
      .notNull()
      .references(() => provider.id, { onDelete: "restrict" }),
    embeddingModelId: text("embedding_model_id").notNull(),
    dimension: integer("dimension").notNull(),
    metric: text("metric").$type<VectorStoreDto["metric"]>().notNull(),
    chunkSize: integer("chunk_size").notNull(),
    chunkOverlap: integer("chunk_overlap").notNull(),
    indexType: text("index_type").notNull().default("FLAT"),
    status: text("status").$type<VectorStoreDto["status"]>().notNull().default("pending"),
    tableCreatedAt: integer("table_created_at"),
    lastIndexedAt: integer("last_indexed_at"),
    documents: integer("documents").notNull().default(0),
    vectors: integer("vectors").notNull().default(0),
    bytes: integer("bytes").notNull().default(0),
    createdAt: integer("created_at").notNull(),
    updatedAt: integer("updated_at").notNull(),
  },
  (table) => [uniqueIndex("vector_store_name_unq").on(table.name)],
);

export type VectorStoreEntity = typeof vectorStore.$inferSelect;
export type VectorStoreInsert = typeof vectorStore.$inferInsert;
