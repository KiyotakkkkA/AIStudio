import { integer, real, sqliteTable, text, uniqueIndex } from "drizzle-orm/sqlite-core";
import type { Json } from "@zvs/shared";
import { graphRelationType } from "./graphRelationType.ts";
import { vectorDocument } from "./vectorDocument.ts";
import { vectorStore } from "./vectorStore.ts";

export const graphEdge = sqliteTable(
  "graph_edge",
  {
    id: text("id").primaryKey(),
    storeId: text("store_id")
      .notNull()
      .references(() => vectorStore.id, { onDelete: "cascade" }),
    fromDocumentId: text("from_document_id")
      .notNull()
      .references(() => vectorDocument.id, { onDelete: "cascade" }),
    toDocumentId: text("to_document_id")
      .notNull()
      .references(() => vectorDocument.id, { onDelete: "cascade" }),
    relationTypeId: text("relation_type_id")
      .notNull()
      .references(() => graphRelationType.id, { onDelete: "restrict" }),
    weight: real("weight"),
    metadata: text("metadata", { mode: "json" }).$type<Json>(),
    createdAt: integer("created_at").notNull(),
  },
  (table) => [
    uniqueIndex("graph_edge_unq").on(
      table.storeId,
      table.fromDocumentId,
      table.toDocumentId,
      table.relationTypeId,
    ),
  ],
);

export type GraphEdgeEntity = typeof graphEdge.$inferSelect;
export type GraphEdgeInsert = typeof graphEdge.$inferInsert;
