import { and, asc, eq, inArray, isNull } from "drizzle-orm";
import type { DatabaseHandle } from "../types.ts";
import { pendingApproval, permissionUse, toolPermission } from "../schema/permission.ts";

export class PermissionRepository {
  constructor(private readonly db: DatabaseHandle) {}
  list(subject: string) {
    return this.db.select().from(toolPermission).where(eq(toolPermission.subject, subject)).all();
  }
  grant(row: typeof toolPermission.$inferInsert) {
    this.db
      .insert(toolPermission)
      .values(row)
      .onConflictDoUpdate({
        target: [toolPermission.subject, toolPermission.scope],
        set: row,
      })
      .run();
  }
  revoke(id: string) {
    this.db.delete(toolPermission).where(eq(toolPermission.id, id)).run();
  }
  log(row: typeof permissionUse.$inferInsert) {
    this.db.insert(permissionUse).values(row).run();
  }
  uses(grantId: string) {
    return this.db.select().from(permissionUse).where(eq(permissionUse.grantId, grantId)).all();
  }
  createApproval(row: typeof pendingApproval.$inferInsert) {
    this.db.insert(pendingApproval).values(row).run();
  }
  approval(id: string) {
    return this.db.select().from(pendingApproval).where(eq(pendingApproval.id, id)).get();
  }
  decide(id: string, decision: "approved" | "denied") {
    this.db.update(pendingApproval).set({ decision }).where(eq(pendingApproval.id, id)).run();
  }
  approvalsForRun(runId: string) {
    return this.db.select().from(pendingApproval).where(eq(pendingApproval.runId, runId)).all();
  }
  pendingForRun(runId: string) {
    return this.db
      .select()
      .from(pendingApproval)
      .where(and(eq(pendingApproval.runId, runId), isNull(pendingApproval.decision)))
      .all();
  }
  pendingForRuns(runIds: readonly string[]) {
    if (runIds.length === 0) return [];
    return this.db
      .select()
      .from(pendingApproval)
      .where(and(inArray(pendingApproval.runId, [...runIds]), isNull(pendingApproval.decision)))
      .orderBy(asc(pendingApproval.expiresAt))
      .all();
  }
  denyUnfinished(runId: string) {
    this.db
      .update(pendingApproval)
      .set({ decision: "denied" })
      .where(and(eq(pendingApproval.runId, runId), isNull(pendingApproval.decision)))
      .run();
  }
}
