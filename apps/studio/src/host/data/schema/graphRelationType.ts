import { integer, sqliteTable, text, uniqueIndex } from "drizzle-orm/sqlite-core";

export const graphRelationType = sqliteTable(
  "graph_relation_type",
  {
    id: text("id").primaryKey(),
    name: text("name").notNull(),
    description: text("description").notNull().default(""),
    directed: integer("directed", { mode: "boolean" }).notNull().default(true),
  },
  (table) => [uniqueIndex("graph_relation_type_name_unq").on(table.name)],
);

export type GraphRelationTypeEntity = typeof graphRelationType.$inferSelect;
export type GraphRelationTypeInsert = typeof graphRelationType.$inferInsert;
