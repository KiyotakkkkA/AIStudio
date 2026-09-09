import { integer, primaryKey, sqliteTable, text } from "drizzle-orm/sqlite-core";
import { secret } from "./secret.ts";

export const SECRET_CONSUMER_KINDS = [
  "provider",
  "mcp_server",
  "integration",
  "vector_store",
  "account",
] as const;
export type SecretConsumerKind = (typeof SECRET_CONSUMER_KINDS)[number];

export const secretUsage = sqliteTable(
  "secret_usage",
  {
    secretId: text("secret_id")
      .notNull()
      .references(() => secret.id, { onDelete: "restrict", onUpdate: "cascade" }),
    consumerKind: text("consumer_kind").$type<SecretConsumerKind>().notNull(),
    consumerId: text("consumer_id").notNull(),
    createdAt: integer("created_at").notNull(),
  },
  (table) => [
    primaryKey({
      name: "secret_usage_pk",
      columns: [table.secretId, table.consumerKind, table.consumerId],
    }),
  ],
);

export type SecretUsageEntity = typeof secretUsage.$inferSelect;
export type SecretUsageInsert = typeof secretUsage.$inferInsert;
