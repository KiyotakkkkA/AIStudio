import { index, integer, sqliteTable, text } from "drizzle-orm/sqlite-core";
import { vectorStore } from "./vectorStore.ts";

export const vectorEvalCase = sqliteTable(
  "vector_eval_case",
  {
    id: text("id").primaryKey(),
    storeId: text("store_id")
      .notNull()
      .references(() => vectorStore.id, { onDelete: "cascade" }),
    question: text("question").notNull(),
    expectedDocumentIds: text("expected_document_ids", { mode: "json" })
      .$type<string[]>()
      .notNull(),
    createdAt: integer("created_at").notNull(),
  },
  (table) => [index("vector_eval_case_store_idx").on(table.storeId, table.createdAt)],
);

export type VectorEvalCaseEntity = typeof vectorEvalCase.$inferSelect;
export type VectorEvalCaseInsert = typeof vectorEvalCase.$inferInsert;
