import { sqliteTable, text, integer } from "drizzle-orm/sqlite-core";

export const setting = sqliteTable("setting", {
  key: text("key").primaryKey(),
  value: text("value").notNull(),
  updatedAt: integer("updated_at").notNull(),
});

export type SettingEntity = typeof setting.$inferSelect;
export type SettingInsert = typeof setting.$inferInsert;
