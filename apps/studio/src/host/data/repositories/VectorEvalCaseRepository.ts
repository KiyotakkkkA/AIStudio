import { and, asc, eq } from "drizzle-orm";
import { vectorEvalCase, type VectorEvalCaseInsert } from "../schema/index.ts";
import { Repository } from "./Repository.ts";

export class VectorEvalCaseRepository extends Repository {
  listByStore(storeId: string) {
    return this.db
      .select()
      .from(vectorEvalCase)
      .where(eq(vectorEvalCase.storeId, storeId))
      .orderBy(asc(vectorEvalCase.createdAt), asc(vectorEvalCase.id))
      .all();
  }
  findInStore(storeId: string, id: string) {
    return this.db
      .select()
      .from(vectorEvalCase)
      .where(and(eq(vectorEvalCase.storeId, storeId), eq(vectorEvalCase.id, id)))
      .get();
  }
  add(draft: VectorEvalCaseInsert) {
    return this.db.insert(vectorEvalCase).values(draft).returning().get();
  }
  remove(storeId: string, id: string): number {
    return this.db
      .delete(vectorEvalCase)
      .where(and(eq(vectorEvalCase.storeId, storeId), eq(vectorEvalCase.id, id)))
      .run().changes;
  }
}
