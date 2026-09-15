import { and, asc, count, eq, or } from "drizzle-orm";
import { graphEdge, type GraphEdgeInsert } from "../schema/index.ts";
import { Repository } from "./Repository.ts";

export interface GraphEdgeLookup {
  storeId: string;
  fromDocumentId: string;
  toDocumentId: string;
  relationTypeId: string;
}

export class GraphEdgeRepository extends Repository {
  listByStore(storeId: string) {
    return this.db
      .select()
      .from(graphEdge)
      .where(eq(graphEdge.storeId, storeId))
      .orderBy(asc(graphEdge.createdAt), asc(graphEdge.id))
      .all();
  }
  listByDocument(storeId: string, documentId: string) {
    return this.db
      .select()
      .from(graphEdge)
      .where(
        and(
          eq(graphEdge.storeId, storeId),
          or(eq(graphEdge.fromDocumentId, documentId), eq(graphEdge.toDocumentId, documentId)),
        ),
      )
      .orderBy(asc(graphEdge.createdAt), asc(graphEdge.id))
      .all();
  }
  findById(id: string) {
    return this.db.select().from(graphEdge).where(eq(graphEdge.id, id)).get();
  }
  findByKey(key: GraphEdgeLookup) {
    return this.db.select().from(graphEdge).where(this.matches(key)).get();
  }
  countByStore(storeId: string): number {
    return (
      this.db.select({ total: count() }).from(graphEdge).where(eq(graphEdge.storeId, storeId)).get()
        ?.total ?? 0
    );
  }
  link(draft: GraphEdgeInsert) {
    return this.db.insert(graphEdge).values(draft).returning().get();
  }
  remove(id: string): number {
    return this.db.delete(graphEdge).where(eq(graphEdge.id, id)).run().changes;
  }
  removeByStore(storeId: string): number {
    return this.db.delete(graphEdge).where(eq(graphEdge.storeId, storeId)).run().changes;
  }
  private matches(key: GraphEdgeLookup) {
    return and(
      eq(graphEdge.storeId, key.storeId),
      eq(graphEdge.fromDocumentId, key.fromDocumentId),
      eq(graphEdge.toDocumentId, key.toDocumentId),
      eq(graphEdge.relationTypeId, key.relationTypeId),
    );
  }
}
