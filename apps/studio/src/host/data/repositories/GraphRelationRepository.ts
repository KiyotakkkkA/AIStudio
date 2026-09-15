import { asc, eq, inArray } from "drizzle-orm";
import { graphRelationType, type GraphRelationTypeInsert } from "../schema/index.ts";
import { Repository } from "./Repository.ts";

export class GraphRelationRepository extends Repository {
  list() {
    return this.db.select().from(graphRelationType).orderBy(asc(graphRelationType.name)).all();
  }
  findById(id: string) {
    return this.db.select().from(graphRelationType).where(eq(graphRelationType.id, id)).get();
  }
  findByName(name: string) {
    return this.db.select().from(graphRelationType).where(eq(graphRelationType.name, name)).get();
  }
  listByIds(ids: readonly string[]) {
    if (ids.length === 0) return [];
    return this.db
      .select()
      .from(graphRelationType)
      .where(inArray(graphRelationType.id, [...ids]))
      .all();
  }
  define(draft: GraphRelationTypeInsert) {
    return this.db.insert(graphRelationType).values(draft).returning().get();
  }
}
