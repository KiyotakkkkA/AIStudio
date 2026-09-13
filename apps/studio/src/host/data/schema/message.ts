import { index, integer, sqliteTable, text } from "drizzle-orm/sqlite-core";
import type { ChatCitationDto } from "@zvs/shared";
import { conversation } from "./conversation.ts";
import { run } from "./run.ts";

export const message = sqliteTable(
  "message",
  {
    id: text("id").primaryKey(),
    conversationId: text("conversation_id")
      .notNull()
      .references(() => conversation.id, { onDelete: "cascade" }),
    role: text("role").$type<"user" | "assistant">().notNull(),
    content: text("content").notNull(),
    citations: text("citations", { mode: "json" }).$type<ChatCitationDto[]>().notNull(),
    tokensIn: integer("tokens_in").notNull(),
    tokensOut: integer("tokens_out").notNull(),
    usageEstimated: integer("usage_estimated", { mode: "boolean" }).notNull().default(true),
    durationMs: integer("duration_ms").notNull(),
    runId: text("run_id").references(() => run.id, { onDelete: "set null" }),
    partial: integer("partial", { mode: "boolean" }).notNull().default(false),
    createdAt: integer("created_at").notNull(),
  },
  (table) => [
    index("message_conversation_created_idx").on(table.conversationId, table.createdAt, table.id),
  ],
);
export type MessageInsert = typeof message.$inferInsert;
