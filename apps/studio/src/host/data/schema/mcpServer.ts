import { index, integer, sqliteTable, text, uniqueIndex } from "drizzle-orm/sqlite-core";
import type {
  Json,
  McpCapabilityKind,
  McpEnv,
  McpHeaders,
  McpServerStatus,
  McpStartup,
  McpTransport,
} from "@zvs/shared";

export const mcpServer = sqliteTable(
  "mcp_server",
  {
    id: text("id").primaryKey(),
    name: text("name").notNull(),
    namespace: text("namespace").notNull(),
    transport: text("transport").$type<McpTransport>().notNull(),
    command: text("command"),
    args: text("args", { mode: "json" }).$type<string[]>().notNull().default([]),
    env: text("env", { mode: "json" }).$type<McpEnv>().notNull().default({}),
    url: text("url"),
    headers: text("headers", { mode: "json" }).$type<McpHeaders>().notNull().default({}),
    enabled: integer("enabled", { mode: "boolean" }).notNull().default(true),
    startup: text("startup").$type<McpStartup>().notNull().default("manual"),
    status: text("status").$type<McpServerStatus>().notNull().default("stopped"),
    protocolVersion: text("protocol_version"),
    serverName: text("server_name"),
    serverVersion: text("server_version"),
    lastError: text("last_error"),
    lastConnectedAt: integer("last_connected_at"),
    createdAt: integer("created_at").notNull(),
    updatedAt: integer("updated_at").notNull(),
  },
  (table) => [
    uniqueIndex("mcp_server_name_unq").on(table.name),
    uniqueIndex("mcp_server_namespace_unq").on(table.namespace),
    index("mcp_server_startup_idx").on(table.startup),
  ],
);

/**
 * Everything a handshake reported. Prompts are stored but not surfaced in v1; keeping them here
 * means the next task reads them rather than re-handshaking to find out they exist.
 */
export const mcpCapability = sqliteTable(
  "mcp_capability",
  {
    id: text("id").primaryKey(),
    serverId: text("server_id")
      .notNull()
      .references(() => mcpServer.id, { onDelete: "cascade" }),
    kind: text("kind").$type<McpCapabilityKind>().notNull(),
    name: text("name").notNull(),
    title: text("title"),
    description: text("description").notNull().default(""),
    toolKey: text("tool_key"),
    uri: text("uri"),
    mimeType: text("mime_type"),
    schema: text("schema", { mode: "json" }).$type<Json>(),
    readOnly: integer("read_only", { mode: "boolean" }).notNull().default(false),
    destructive: integer("destructive", { mode: "boolean" }).notNull().default(false),
    discoveredAt: integer("discovered_at").notNull(),
  },
  (table) => [
    uniqueIndex("mcp_capability_unq").on(table.serverId, table.kind, table.name),
    index("mcp_capability_tool_key_idx").on(table.toolKey),
  ],
);

export type McpServerEntity = typeof mcpServer.$inferSelect;
export type McpServerInsert = typeof mcpServer.$inferInsert;
export type McpCapabilityEntity = typeof mcpCapability.$inferSelect;
export type McpCapabilityInsert = typeof mcpCapability.$inferInsert;
