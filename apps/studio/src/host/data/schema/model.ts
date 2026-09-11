import { integer, sqliteTable, text, uniqueIndex } from "drizzle-orm/sqlite-core";
import { MODEL_CAPABILITIES, type ModelCapability } from "@zvs/shared";
import { provider } from "./provider.ts";

export { MODEL_CAPABILITIES };
export type { ModelCapability };

export const model = sqliteTable(
  "model",
  {
    id: text("id").primaryKey(),
    providerId: text("provider_id")
      .notNull()
      .references(() => provider.id, { onDelete: "cascade" }),
    externalId: text("external_id").notNull(),
    isFree: integer("is_free", { mode: "boolean" }),
    noTraining: integer("no_training", { mode: "boolean" }),
    displayName: text("display_name").notNull(),
    family: text("family"),
    contextWindow: integer("context_window"),
    maxOutput: integer("max_output"),
    sizeBytes: integer("size_bytes"),
    capabilities: text("capabilities", { mode: "json" })
      .$type<ModelCapability[]>()
      .notNull()
      .default([]),
    available: integer("available", { mode: "boolean" }).notNull().default(true),
    unavailableReason: text("unavailable_reason"),
    discoveredAt: integer("discovered_at").notNull(),
  },
  (table) => [uniqueIndex("model_provider_external_unq").on(table.providerId, table.externalId)],
);

export type ModelEntity = typeof model.$inferSelect;
export type ModelInsert = typeof model.$inferInsert;
