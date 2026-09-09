import { index, integer, sqliteTable, text, uniqueIndex } from "drizzle-orm/sqlite-core";
import { secret } from "./secret.ts";

export const PROVIDER_KINDS = [
  "ollama",
  "openrouter",
  "anthropic",
  "mistral",
  "openai-compatible",
] as const;
export type ProviderKind = (typeof PROVIDER_KINDS)[number];
export const PROVIDER_CAPABILITIES = ["text", "embedding", "image"] as const;
export type ProviderCapability = (typeof PROVIDER_CAPABILITIES)[number];
export const PROVIDER_STATUSES = ["unknown", "ok", "degraded", "failed"] as const;
export type ProviderStatus = (typeof PROVIDER_STATUSES)[number];

export interface ProviderSettings {
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
    name: text("name").notNull(),
    baseUrl: text("base_url").notNull(),
    secretId: text("secret_id").references(() => secret.id, { onDelete: "restrict" }),
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
