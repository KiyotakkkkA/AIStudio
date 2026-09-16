import { and, asc, eq, inArray, notInArray } from "drizzle-orm";
import type { McpCapabilityKind } from "@zvs/shared";
import {
  mcpCapability,
  mcpServer,
  type McpCapabilityEntity,
  type McpCapabilityInsert,
  type McpServerEntity,
  type McpServerInsert,
} from "../schema/index.ts";
import { Repository } from "./Repository.ts";

export type McpServerPatch = Partial<Omit<McpServerInsert, "id" | "createdAt">>;

export class McpServerRepository extends Repository {
  list(): McpServerEntity[] {
    return this.db.select().from(mcpServer).orderBy(asc(mcpServer.name)).all();
  }

  findById(id: string): McpServerEntity | undefined {
    return this.db.select().from(mcpServer).where(eq(mcpServer.id, id)).get();
  }

  findByNamespace(namespace: string): McpServerEntity | undefined {
    return this.db.select().from(mcpServer).where(eq(mcpServer.namespace, namespace)).get();
  }

  /** The servers the composition root brings up without waiting for a call to need them. */
  listAutoStart(): McpServerEntity[] {
    return this.db
      .select()
      .from(mcpServer)
      .where(and(eq(mcpServer.enabled, true), eq(mcpServer.startup, "launch")))
      .orderBy(asc(mcpServer.name))
      .all();
  }

  insert(row: McpServerInsert): McpServerEntity {
    return this.db.insert(mcpServer).values(row).returning().get();
  }

  update(id: string, patch: McpServerPatch): McpServerEntity {
    return this.db.update(mcpServer).set(patch).where(eq(mcpServer.id, id)).returning().get();
  }

  remove(id: string): void {
    this.db.delete(mcpServer).where(eq(mcpServer.id, id)).run();
  }

  capabilities(serverId: string): McpCapabilityEntity[] {
    return this.db
      .select()
      .from(mcpCapability)
      .where(eq(mcpCapability.serverId, serverId))
      .orderBy(asc(mcpCapability.kind), asc(mcpCapability.name))
      .all();
  }

  capabilitiesOfKind(serverId: string, kind: McpCapabilityKind): McpCapabilityEntity[] {
    return this.db
      .select()
      .from(mcpCapability)
      .where(and(eq(mcpCapability.serverId, serverId), eq(mcpCapability.kind, kind)))
      .orderBy(asc(mcpCapability.name))
      .all();
  }

  findCapabilityByToolKey(toolKey: string): McpCapabilityEntity | undefined {
    return this.db.select().from(mcpCapability).where(eq(mcpCapability.toolKey, toolKey)).get();
  }

  listToolCapabilities(): McpCapabilityEntity[] {
    return this.db
      .select()
      .from(mcpCapability)
      .where(eq(mcpCapability.kind, "tool"))
      .orderBy(asc(mcpCapability.name))
      .all();
  }

  upsertCapability(row: McpCapabilityInsert): McpCapabilityEntity {
    return this.db
      .insert(mcpCapability)
      .values(row)
      .onConflictDoUpdate({
        target: [mcpCapability.serverId, mcpCapability.kind, mcpCapability.name],
        set: {
          title: row.title ?? null,
          description: row.description ?? "",
          toolKey: row.toolKey ?? null,
          uri: row.uri ?? null,
          mimeType: row.mimeType ?? null,
          schema: row.schema ?? null,
          readOnly: row.readOnly ?? false,
          destructive: row.destructive ?? false,
          discoveredAt: row.discoveredAt,
        },
      })
      .returning()
      .get();
  }

  /** Drops whatever the latest handshake no longer reports, so the list never drifts upward. */
  pruneCapabilities(serverId: string, keepIds: readonly string[]): void {
    const owned = eq(mcpCapability.serverId, serverId);
    this.db
      .delete(mcpCapability)
      .where(keepIds.length ? and(owned, notInArray(mcpCapability.id, [...keepIds])) : owned)
      .run();
  }

  capabilityCounts(serverId: string): Record<McpCapabilityKind, number> {
    const counts: Record<McpCapabilityKind, number> = { tool: 0, resource: 0, prompt: 0 };
    for (const row of this.capabilities(serverId)) counts[row.kind] += 1;
    return counts;
  }

  countsForServers(serverIds: readonly string[]): Map<string, Record<McpCapabilityKind, number>> {
    const counts = new Map<string, Record<McpCapabilityKind, number>>();
    if (serverIds.length === 0) return counts;
    const rows = this.db
      .select()
      .from(mcpCapability)
      .where(inArray(mcpCapability.serverId, [...serverIds]))
      .all();
    for (const row of rows) {
      const entry = counts.get(row.serverId) ?? { tool: 0, resource: 0, prompt: 0 };
      entry[row.kind] += 1;
      counts.set(row.serverId, entry);
    }
    return counts;
  }
}
