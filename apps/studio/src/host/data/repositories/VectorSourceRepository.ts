import { asc, eq } from "drizzle-orm";
import { vectorSource, type VectorSourceInsert } from "../schema/index.ts";
import { Repository } from "./Repository.ts";

export class VectorSourceRepository extends Repository {
  listByStore(storeId: string) {
    return this.db
      .select()
      .from(vectorSource)
      .where(eq(vectorSource.storeId, storeId))
      .orderBy(asc(vectorSource.path))
      .all();
  }
  findById(id: string) {
    return this.db.select().from(vectorSource).where(eq(vectorSource.id, id)).get();
  }
  add(draft: VectorSourceInsert) {
    return this.db
      .insert(vectorSource)
      .values(draft)
      .onConflictDoUpdate({
        target: [vectorSource.storeId, vectorSource.path],
        set: {
          kind: draft.kind,
          include: draft.include,
          exclude: draft.exclude,
          recursive: draft.recursive ?? true,
        },
      })
      .returning()
      .get();
  }
  remove(id: string): void {
    this.db.delete(vectorSource).where(eq(vectorSource.id, id)).run();
  }
}
