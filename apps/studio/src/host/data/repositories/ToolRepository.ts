import { and, asc, eq, inArray, isNull, notInArray, sql } from "drizzle-orm";
import type { ToolReferenceKind, ToolSource } from "@zvs/shared";
import {
  tool,
  toolReference,
  type ToolEntity,
  type ToolInsert,
  type ToolReferenceEntity,
  type ToolReferenceInsert,
} from "../schema/index.ts";
import { Repository } from "./Repository.ts";

export type ToolPatch = Partial<Omit<ToolInsert, "id" | "key">>;

export class ToolRepository extends Repository {
  list(): ToolEntity[] {
    return this.db.select().from(tool).orderBy(asc(tool.key)).all();
  }
  findById(id: string): ToolEntity | undefined {
    return this.db.select().from(tool).where(eq(tool.id, id)).get();
  }
  findByKey(key: string): ToolEntity | undefined {
    return this.db.select().from(tool).where(eq(tool.key, key)).get();
  }
  listBySource(source: ToolSource, sourceRef: string | null): ToolEntity[] {
    return this.db
      .select()
      .from(tool)
      .where(
        and(
          eq(tool.source, source),
          sourceRef === null ? isNull(tool.sourceRef) : eq(tool.sourceRef, sourceRef),
        ),
      )
      .orderBy(asc(tool.key))
      .all();
  }
  insert(row: ToolInsert): ToolEntity {
    return this.db.insert(tool).values(row).returning().get();
  }
  update(id: string, patch: ToolPatch): ToolEntity {
    return this.db.update(tool).set(patch).where(eq(tool.id, id)).returning().get();
  }
  recordUse(id: string, usedAt: number): void {
    this.db
      .update(tool)
      .set({ lastUsedAt: usedAt, callCount: sql`${tool.callCount} + 1` })
      .where(eq(tool.id, id))
      .run();
  }
  references(toolId: string): ToolReferenceEntity[] {
    return this.db
      .select()
      .from(toolReference)
      .where(eq(toolReference.toolId, toolId))
      .orderBy(asc(toolReference.kind), asc(toolReference.name))
      .all();
  }
  linkReference(row: ToolReferenceInsert): void {
    this.db
      .insert(toolReference)
      .values(row)
      .onConflictDoUpdate({
        target: [toolReference.toolId, toolReference.kind, toolReference.refId],
        set: { name: row.name },
      })
      .run();
  }
  clearReferences(kind: ToolReferenceKind, refId: string, keepToolIds: readonly string[]): void {
    const matches = and(eq(toolReference.kind, kind), eq(toolReference.refId, refId));
    this.db
      .delete(toolReference)
      .where(
        keepToolIds.length
          ? and(matches, notInArray(toolReference.toolId, [...keepToolIds]))
          : matches,
      )
      .run();
  }
  referencesFor(toolIds: readonly string[]): ToolReferenceEntity[] {
    if (toolIds.length === 0) return [];
    return this.db
      .select()
      .from(toolReference)
      .where(inArray(toolReference.toolId, [...toolIds]))
      .all();
  }
}
