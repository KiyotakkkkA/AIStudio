import { and, asc, eq } from "drizzle-orm";
import { vectorDocument, type VectorDocumentInsert } from "../schema/index.ts";
import { Repository } from "./Repository.ts";

export class VectorDocumentRepository extends Repository {
  listByStore(storeId: string) {
    return this.db
      .select()
      .from(vectorDocument)
      .where(eq(vectorDocument.storeId, storeId))
      .orderBy(asc(vectorDocument.sourcePath))
      .all();
  }
  findById(id: string) {
    return this.db.select().from(vectorDocument).where(eq(vectorDocument.id, id)).get();
  }
  findBySource(storeId: string, sourcePath: string) {
    return this.db
      .select()
      .from(vectorDocument)
      .where(and(eq(vectorDocument.storeId, storeId), eq(vectorDocument.sourcePath, sourcePath)))
      .get();
  }
  recordIndexed(draft: VectorDocumentInsert) {
    return this.db
      .insert(vectorDocument)
      .values(draft)
      .onConflictDoUpdate({
        target: [vectorDocument.storeId, vectorDocument.sourcePath],
        set: {
          contentHash: draft.contentHash,
          chunkCount: draft.chunkCount,
          bytes: draft.bytes,
          indexedAt: draft.indexedAt,
        },
      })
      .returning()
      .get();
  }
  remove(id: string): void {
    this.db.delete(vectorDocument).where(eq(vectorDocument.id, id)).run();
  }
  removeBySource(storeId: string, sourcePath: string): void {
    this.db
      .delete(vectorDocument)
      .where(and(eq(vectorDocument.storeId, storeId), eq(vectorDocument.sourcePath, sourcePath)))
      .run();
  }
  removeByStore(storeId: string): number {
    return this.db.delete(vectorDocument).where(eq(vectorDocument.storeId, storeId)).run().changes;
  }
}
