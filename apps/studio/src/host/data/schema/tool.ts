import { integer, sqliteTable, text, uniqueIndex } from "drizzle-orm/sqlite-core";
import type { Json, ToolReferenceKind, ToolSource, ToolTier } from "@zvs/shared";

export const tool = sqliteTable(
  "tool",
  {
    id: text("id").primaryKey(),
    key: text("key").notNull(),
    source: text("source").$type<ToolSource>().notNull(),
    sourceRef: text("source_ref"),
    displayName: text("display_name").notNull(),
    description: text("description").notNull().default(""),
    inputSchema: text("input_schema", { mode: "json" }).$type<Json>(),
    readOnly: integer("read_only", { mode: "boolean" }).notNull().default(false),
    destructive: integer("destructive", { mode: "boolean" }).notNull().default(false),
    tier: text("tier").$type<ToolTier>().notNull(),
    enabled: integer("enabled", { mode: "boolean" }).notNull().default(true),
    available: integer("available", { mode: "boolean" }).notNull().default(true),
    lastUsedAt: integer("last_used_at"),
    callCount: integer("call_count").notNull().default(0),
    registeredAt: integer("registered_at").notNull(),
    updatedAt: integer("updated_at").notNull(),
  },
  (table) => [uniqueIndex("tool_key_unq").on(table.key)],
);

export const toolReference = sqliteTable(
  "tool_reference",
  {
    id: text("id").primaryKey(),
    toolId: text("tool_id")
      .notNull()
      .references(() => tool.id, { onDelete: "cascade" }),
    kind: text("kind").$type<ToolReferenceKind>().notNull(),
    refId: text("ref_id").notNull(),
    name: text("name").notNull(),
  },
  (table) => [uniqueIndex("tool_reference_unq").on(table.toolId, table.kind, table.refId)],
);

export type ToolEntity = typeof tool.$inferSelect;
export type ToolInsert = typeof tool.$inferInsert;
export type ToolReferenceEntity = typeof toolReference.$inferSelect;
export type ToolReferenceInsert = typeof toolReference.$inferInsert;
