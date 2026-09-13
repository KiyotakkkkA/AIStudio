import { asc, eq, sql } from "drizzle-orm";
import { AppError, AppErrorCode } from "@zvs/shared";
import { model, provider, type ModelEntity, type ModelInsert } from "../schema/index.ts";
import { createId } from "../../platform/ids.ts";
import { Repository } from "./Repository.ts";

export type DiscoveredModel = Omit<ModelInsert, "id" | "providerId">;

export class ModelRepository extends Repository {
  listByProvider(providerId: string, selectedOnly = false): ModelEntity[] {
    if (selectedOnly) {
      return this.db
        .select({ model })
        .from(model)
        .innerJoin(provider, eq(model.providerId, provider.id))
        .where(
          sql`${provider.id} = ${providerId} AND EXISTS (SELECT 1 FROM json_each(${provider.settings}, '$.selectedModelIds') WHERE value = ${model.externalId})`,
        )
        .orderBy(asc(model.externalId))
        .all()
        .map((row) => row.model);
    }
    return this.db
      .select()
      .from(model)
      .where(eq(model.providerId, providerId))
      .orderBy(asc(model.externalId))
      .all();
  }

  findById(id: string): ModelEntity | undefined {
    return this.db.select().from(model).where(eq(model.id, id)).get();
  }

  replaceForProvider(providerId: string, models: DiscoveredModel[]): ModelEntity[] {
    return this.db.transaction((tx) => {
      const owner = tx.select().from(provider).where(eq(provider.id, providerId)).get();
      if (owner === undefined) throw new AppError(AppErrorCode.NOT_FOUND, "Провайдер не найден");
      const repository = new ModelRepository(tx);
      const previous = new Map(
        repository.listByProvider(providerId).map((row) => [row.externalId, row.id]),
      );
      tx.delete(model).where(eq(model.providerId, providerId)).run();
      for (const draft of models) {
        tx.insert(model)
          .values({ ...draft, providerId, id: previous.get(draft.externalId) ?? createId() })
          .run();
      }
      const rows = repository.listByProvider(providerId);
      if (owner.defaultModelId !== null && !rows.some((row) => row.id === owner.defaultModelId)) {
        tx.update(provider)
          .set({ defaultModelId: null, updatedAt: Date.now() })
          .where(eq(provider.id, providerId))
          .run();
      }
      return rows;
    });
  }
}
