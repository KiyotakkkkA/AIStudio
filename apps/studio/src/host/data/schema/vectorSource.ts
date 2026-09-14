import { integer, sqliteTable, text, uniqueIndex } from "drizzle-orm/sqlite-core";
import type { VectorSourceKind } from "@zvs/shared";
import { vectorStore } from "./vectorStore.ts";

export const vectorSource = sqliteTable(
  "vector_source",
  {
    id: text("id").primaryKey(),
    storeId: text("store_id")
      .notNull()
      .references(() => vectorStore.id, { onDelete: "cascade" }),
    kind: text("kind").$type<VectorSourceKind>().notNull(),
    path: text("path").notNull(),
    include: text("include", { mode: "json" }).$type<string[]>().notNull(),
    exclude: text("exclude", { mode: "json" }).$type<string[]>().notNull(),
    recursive: integer("recursive", { mode: "boolean" }).notNull().default(true),
    createdAt: integer("created_at").notNull(),
  },
  (table) => [uniqueIndex("vector_source_store_path_unq").on(table.storeId, table.path)],
);

export type VectorSourceEntity = typeof vectorSource.$inferSelect;
export type VectorSourceInsert = typeof vectorSource.$inferInsert;
