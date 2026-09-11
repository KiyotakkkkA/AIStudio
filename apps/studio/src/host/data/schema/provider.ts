import { index, integer, sqliteTable, text, uniqueIndex } from "drizzle-orm/sqlite-core";
import {
  PROVIDER_CAPABILITIES,
  PROVIDER_KINDS,
  PROVIDER_STATUSES,
  type AdapterFamily,
  type AuthMode,
  type ProviderCapability,
  type ProviderKind,
  type ProviderStatus,
} from "@zvs/shared";
import { account } from "./account.ts";
import { secret } from "./secret.ts";

export { PROVIDER_CAPABILITIES, PROVIDER_KINDS, PROVIDER_STATUSES };
export type { ProviderCapability, ProviderKind, ProviderStatus };

export interface ProviderSettings {
  selectedModelIds?: string[];
  temperature?: number;
  topK?: number;
  topP?: number;
  maxOutputTokens?: number;
  timeoutSeconds?: number;
}

export const provider = sqliteTable(
  "provider",
  {
    id: text("id").primaryKey(),
    kind: text("kind").$type<ProviderKind>().notNull(),
    adapter: text("adapter").$type<AdapterFamily>().notNull().default("openai-compatible"),
    authMode: text("auth_mode").$type<AuthMode>().notNull().default("api"),
    name: text("name").notNull(),
    baseUrl: text("base_url").notNull(),
    secretId: text("secret_id").references(() => secret.id, { onDelete: "restrict" }),
    accountId: text("account_id").references(() => account.id, { onDelete: "set null" }),
    capabilities: text("capabilities", { mode: "json" }).$type<ProviderCapability[]>().notNull(),
    capText: integer("cap_text", { mode: "boolean" }).notNull().default(false),
    capEmbedding: integer("cap_embedding", { mode: "boolean" }).notNull().default(false),
    capImage: integer("cap_image", { mode: "boolean" }).notNull().default(false),
    enabled: integer("enabled", { mode: "boolean" }).notNull().default(true),
    settings: text("settings", { mode: "json" }).$type<ProviderSettings>().notNull().default({}),
    defaultModelId: text("default_model_id"),
    status: text("status").$type<ProviderStatus>().notNull().default("unknown"),
    statusDetail: text("status_detail"),
    lastProbeAt: integer("last_probe_at"),
    lastLatencyMs: integer("last_latency_ms"),
    createdAt: integer("created_at").notNull(),
    updatedAt: integer("updated_at").notNull(),
  },
  (table) => [
    uniqueIndex("provider_kind_name_unq").on(table.kind, table.name),
    index("provider_cap_text_idx").on(table.capText),
    index("provider_cap_embedding_idx").on(table.capEmbedding),
    index("provider_cap_image_idx").on(table.capImage),
  ],
);

export type ProviderEntity = typeof provider.$inferSelect;
export type ProviderInsert = typeof provider.$inferInsert;
