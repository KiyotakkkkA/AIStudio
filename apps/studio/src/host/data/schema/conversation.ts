import { integer, sqliteTable, text } from "drizzle-orm/sqlite-core";
import type { ChatSettings } from "@zvs/shared";

export const conversation = sqliteTable("conversation", {
  id: text("id").primaryKey(),
  title: text("title").notNull(),
  providerId: text("provider_id").notNull(),
  modelId: text("model_id").notNull(),
  settings: text("settings", { mode: "json" }).$type<ChatSettings>().notNull(),
  systemPrompt: text("system_prompt").notNull(),
  attachedStoreIds: text("attached_store_ids", { mode: "json" }).$type<string[]>().notNull(),
  createdAt: integer("created_at").notNull(),
  updatedAt: integer("updated_at").notNull(),
});
export type ConversationInsert = typeof conversation.$inferInsert;
