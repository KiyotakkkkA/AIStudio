import { z } from "zod";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import type { PermissionTier } from "../data/schema/permission.ts";
import type { NodeRegistry } from "../kernel/NodeRegistry.ts";
import { createId } from "../platform/ids.ts";

const Scope = z.union([z.literal("global"), z.string().regex(/^(skill|scenario|site):.+$/)]);
const rank = { auto: 0, ask: 1, off: 2 } as const;
export interface PermissionContext {
  runId: string;
  scopes: string[];
}
export class PermissionService {
  constructor(
    private readonly data: UnitOfWork,
    private readonly registry: NodeRegistry,
    private readonly clock: () => number = Date.now,
  ) {}
  grant(subject: string, scope: string, tier: PermissionTier, grantedBy: string) {
    z.string().min(1).parse(subject);
    Scope.parse(scope);
    z.enum(["auto", "ask", "off"]).parse(tier);
    z.string().min(1).parse(grantedBy);
    const id = createId();
    this.data.repositories.permissions.grant({
      id,
      subject,
      scope,
      tier,
      grantedBy,
      grantedAt: this.clock(),
    });
    return id;
  }
  revoke(id: string) {
    this.data.repositories.permissions.revoke(id);
  }
  admit(nodeType: string, context: PermissionContext): "allow" | "ask" | "deny" {
    const requirement = this.registry.resolve(nodeType).permission;
    if ("kind" in requirement) return "allow";
    return this.admitSubject(requirement.tool, context, requirement.tier);
  }
  admitSubject(
    subject: string,
    context: PermissionContext,
    minimum: PermissionTier = "auto",
  ): "allow" | "ask" | "deny" {
    const rows = this.data.repositories.permissions.list(subject);
    const ceiling = rows.find((row) => row.scope === "global");
    let tier: PermissionTier = ceiling?.tier ?? "ask";
    if (rank[minimum] > rank[tier]) tier = minimum;
    const applicable = rows.filter(
      (row) => row.scope === "global" || context.scopes.includes(row.scope),
    );
    for (const row of applicable) if (rank[row.tier] > rank[tier]) tier = row.tier;
    if (tier === "auto")
      for (const row of applicable)
        this.data.repositories.permissions.log({
          id: createId(),
          grantId: row.id,
          runId: context.runId,
          usedAt: this.clock(),
          subject: row.subject,
          scope: row.scope,
          tier: row.tier,
          grantedBy: row.grantedBy,
          grantedAt: row.grantedAt,
        });
    return tier === "auto" ? "allow" : tier === "ask" ? "ask" : "deny";
  }
}
