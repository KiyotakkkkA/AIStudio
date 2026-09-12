import { integer, sqliteTable, text, uniqueIndex } from "drizzle-orm/sqlite-core";

export type PermissionTier = "auto" | "ask" | "off";
export const toolPermission = sqliteTable(
  "tool_permission",
  {
    id: text("id").primaryKey(),
    subject: text("subject").notNull(),
    scope: text("scope").notNull(),
    tier: text("tier").$type<PermissionTier>().notNull(),
    grantedAt: integer("granted_at").notNull(),
    grantedBy: text("granted_by").notNull(),
  },
  (t) => [uniqueIndex("tool_permission_subject_scope").on(t.subject, t.scope)],
);
export const permissionUse = sqliteTable("permission_use", {
  id: text("id").primaryKey(),
  grantId: text("grant_id").notNull(),
  runId: text("run_id").notNull(),
  usedAt: integer("used_at").notNull(),
  subject: text("subject").notNull(),
  scope: text("scope").notNull(),
  tier: text("tier").$type<PermissionTier>().notNull(),
  grantedBy: text("granted_by").notNull(),
  grantedAt: integer("granted_at").notNull(),
});
export const pendingApproval = sqliteTable("pending_approval", {
  id: text("id").primaryKey(),
  runId: text("run_id").notNull(),
  subject: text("subject").notNull(),
  scope: text("scope").notNull(),
  expiresAt: integer("expires_at").notNull(),
  decision: text("decision").$type<"approved" | "denied">(),
});
