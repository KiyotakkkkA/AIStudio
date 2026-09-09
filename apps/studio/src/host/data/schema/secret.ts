import { blob, index, integer, sqliteTable, text, uniqueIndex } from "drizzle-orm/sqlite-core";

export const SECRET_TYPES = ["ollama", "openrouter", "mistral", "custom"] as const;
export type KnownSecretType = (typeof SECRET_TYPES)[number];

export const SECRET_SCOPES = ["personal", "shared", "public"] as const;
export type SecretScope = (typeof SECRET_SCOPES)[number];

export const secret = sqliteTable(
  "secret",
  {
    id: text("id").primaryKey(),
    type: text("type").notNull(),
    name: text("name").notNull(),
    scope: text("scope").$type<SecretScope>().notNull(),
    cipher: blob("cipher", { mode: "buffer" }),
    cipherVersion: integer("cipher_version").notNull().default(1),
    hint: text("hint"),
    fields: text("fields").notNull().default("{}"),
    tags: text("tags").notNull().default("[]"),
    note: text("note"),
    rotationDays: integer("rotation_days"),
    rotatesAt: integer("rotates_at"),
    createdAt: integer("created_at").notNull(),
    updatedAt: integer("updated_at").notNull(),
  },
  (table) => [
    index("secret_type_idx").on(table.type),
    index("secret_scope_idx").on(table.scope),
    uniqueIndex("secret_name_type_unq").on(table.name, table.type),
  ],
);

export type SecretEntity = typeof secret.$inferSelect;
export type SecretInsert = typeof secret.$inferInsert;
export type SecretSummary = Omit<SecretEntity, "cipher">;
export type SecretCipher = Pick<SecretEntity, "id" | "cipher" | "cipherVersion">;
