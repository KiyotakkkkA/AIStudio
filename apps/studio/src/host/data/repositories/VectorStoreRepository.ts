import { asc, eq } from "drizzle-orm";
import { vectorStore, type VectorStoreInsert } from "../schema/index.ts";
import { Repository } from "./Repository.ts";

export class VectorStoreRepository extends Repository {
  list() {
    return this.db
      .select()
      .from(vectorStore)
      .orderBy(asc(vectorStore.name), asc(vectorStore.id))
      .all();
  }
  findById(id: string) {
    return this.db.select().from(vectorStore).where(eq(vectorStore.id, id)).get();
  }
  findByName(name: string) {
    return this.db.select().from(vectorStore).where(eq(vectorStore.name, name)).get();
  }
  create(draft: VectorStoreInsert) {
    return this.db.insert(vectorStore).values(draft).returning().get();
  }
  update(id: string, patch: Partial<Omit<VectorStoreInsert, "id" | "createdAt">>) {
    return this.db.update(vectorStore).set(patch).where(eq(vectorStore.id, id)).returning().get();
  }
  remove(id: string): void {
    this.db.delete(vectorStore).where(eq(vectorStore.id, id)).run();
  }
}
