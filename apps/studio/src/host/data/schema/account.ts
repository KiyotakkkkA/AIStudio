import { index, integer, sqliteTable, text, uniqueIndex } from "drizzle-orm/sqlite-core";
import type { AccountFamily, AccountStatus } from "@zvs/shared";
import { secret } from "./secret.ts";

export const account = sqliteTable(
  "account",
  {
    id: text("id").primaryKey(),
    adapter: text("adapter").$type<AccountFamily>().notNull(),
    partition: text("partition").notNull(),
    externalId: text("external_id").notNull(),
    emailMasked: text("email_masked"),
    displayName: text("display_name"),
    avatarUrl: text("avatar_url"),
    tokenSecretId: text("token_secret_id").references(() => secret.id, { onDelete: "set null" }),
    tokenExpiresAt: integer("token_expires_at"),
    status: text("status").$type<AccountStatus>().notNull().default("linked"),
    statusDetail: text("status_detail"),
    lastCheckedAt: integer("last_checked_at"),
    linkedAt: integer("linked_at").notNull(),
    updatedAt: integer("updated_at").notNull(),
  },
  (table) => [
    uniqueIndex("account_adapter_external_unq").on(table.adapter, table.externalId),
    index("account_adapter_idx").on(table.adapter),
  ],
);

export type AccountEntity = typeof account.$inferSelect;
export type AccountInsert = typeof account.$inferInsert;
