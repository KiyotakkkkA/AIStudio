import { and, asc, desc, eq, inArray, notInArray } from "drizzle-orm";
import type { DownloadStatus } from "@zvs/shared";
import { download, type DownloadEntity, type DownloadInsert } from "../schema/index.ts";
import { Repository } from "./Repository.ts";

const ACTIVE: readonly DownloadStatus[] = ["queued", "running", "paused"];

export type DownloadPatch = Partial<Omit<DownloadInsert, "id" | "createdAt">>;

export class DownloadRepository extends Repository {
  list(): DownloadEntity[] {
    return this.db.select().from(download).orderBy(desc(download.createdAt)).all();
  }

  findById(id: string): DownloadEntity | undefined {
    return this.db.select().from(download).where(eq(download.id, id)).get();
  }

  /** The one unfinished download for an item, if any. Starting a second is a conflict. */
  findActiveByRef(itemRef: string): DownloadEntity | undefined {
    return this.db
      .select()
      .from(download)
      .where(and(eq(download.itemRef, itemRef), inArray(download.status, [...ACTIVE])))
      .get();
  }

  findLatestByRef(itemRef: string): DownloadEntity | undefined {
    return this.db
      .select()
      .from(download)
      .where(eq(download.itemRef, itemRef))
      .orderBy(desc(download.createdAt))
      .get();
  }

  byStatus(statuses: readonly DownloadStatus[]): DownloadEntity[] {
    return this.db
      .select()
      .from(download)
      .where(inArray(download.status, [...statuses]))
      .orderBy(desc(download.createdAt))
      .all();
  }

  /**
   * The queue order: priority first, then the smallest item, so a 22 MB MCP package is never
   * stuck behind a 240 GB model that happened to be queued first.
   */
  nextQueued(limit: number): DownloadEntity[] {
    return this.db
      .select()
      .from(download)
      .where(eq(download.status, "queued"))
      .orderBy(desc(download.priority), asc(download.sizeBytes), asc(download.createdAt))
      .limit(limit)
      .all();
  }

  running(): DownloadEntity[] {
    return this.db.select().from(download).where(eq(download.status, "running")).all();
  }

  installed(): DownloadEntity[] {
    return this.db.select().from(download).where(eq(download.status, "succeeded")).all();
  }

  create(draft: DownloadInsert): DownloadEntity {
    return this.db.insert(download).values(draft).returning().get();
  }

  update(id: string, patch: DownloadPatch): DownloadEntity {
    return this.db.update(download).set(patch).where(eq(download.id, id)).returning().get();
  }

  remove(id: string): void {
    this.db.delete(download).where(eq(download.id, id)).run();
  }

  /**
   * Marks whatever was mid-flight when the app stopped as paused: the partial files are still
   * on disk, so the queue picks them up again rather than starting over.
   */
  interrupt(at: number): number {
    return this.db
      .update(download)
      .set({ status: "paused", runId: null, updatedAt: at })
      .where(eq(download.status, "running"))
      .returning()
      .all().length;
  }

  removeTerminal(): number {
    return this.db
      .delete(download)
      .where(notInArray(download.status, [...ACTIVE]))
      .returning()
      .all().length;
  }
}
