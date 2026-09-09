import { and, asc, eq, type SQL } from "drizzle-orm";
import { AppError, AppErrorCode } from "@zvs/shared";
import {
  model,
  provider,
  type ProviderCapability,
  type ProviderEntity,
  type ProviderInsert,
  type ProviderStatus,
} from "../schema/index.ts";
import { createId } from "../../platform/ids.ts";
import { Repository } from "./Repository.ts";
import { SecretRepository } from "./SecretRepository.ts";

type DerivedColumns = "capText" | "capEmbedding" | "capImage";
export type ProviderDraft = Omit<ProviderInsert, "id" | DerivedColumns | "defaultModelId"> & {
  id?: string;
};
export type ProviderPatch = Partial<Omit<ProviderDraft, "id" | "createdAt">>;
export interface ProviderFilter {
  capability?: ProviderCapability;
  enabled?: boolean;
}

export class ProviderRepository extends Repository {
  list(filter: ProviderFilter = {}): ProviderEntity[] {
    const conditions: SQL[] = [];
    if (filter.capability !== undefined) {
      const columns = {
        text: provider.capText,
        embedding: provider.capEmbedding,
        image: provider.capImage,
      };
      conditions.push(eq(columns[filter.capability], true));
    }
    if (filter.enabled !== undefined) conditions.push(eq(provider.enabled, filter.enabled));
    return this.db
      .select()
      .from(provider)
      .where(and(...conditions))
      .orderBy(asc(provider.name), asc(provider.id))
      .all();
  }

  findById(id: string): ProviderEntity | undefined {
    return this.db.select().from(provider).where(eq(provider.id, id)).get();
  }

  create(draft: ProviderDraft): ProviderEntity {
    return this.db.transaction((tx) => {
      const row = tx
        .insert(provider)
        .values({ ...draft, ...capabilityColumns(draft.capabilities), id: draft.id ?? createId() })
        .returning()
        .get();
      requireAuthModeIntegrity(row);
      if (row.secretId !== null)
        new SecretRepository(tx).addUsage({
          secretId: row.secretId,
          consumerKind: "provider",
          consumerId: row.id,
          createdAt: row.createdAt,
        });
      return row;
    });
  }

  update(id: string, patch: ProviderPatch): ProviderEntity | undefined {
    return this.db.transaction((tx) => {
      const previous = new ProviderRepository(tx).findById(id);
      if (previous === undefined || Object.keys(patch).length === 0) return previous;
      const row = tx
        .update(provider)
        .set({
          ...patch,
          ...capabilityColumns(patch.capabilities ?? previous.capabilities),
          updatedAt: patch.updatedAt ?? Date.now(),
        })
        .where(eq(provider.id, id))
        .returning()
        .get();
      if (row !== undefined) requireAuthModeIntegrity(row);
      if (row !== undefined && previous.secretId !== row.secretId) {
        const secrets = new SecretRepository(tx);
        if (previous.secretId !== null) secrets.removeUsage(previous.secretId, "provider", id);
        if (row.secretId !== null)
          secrets.addUsage({
            secretId: row.secretId,
            consumerKind: "provider",
            consumerId: id,
            createdAt: row.updatedAt,
          });
      }
      return row;
    });
  }

  remove(id: string): void {
    this.db.transaction((tx) => {
      const row = new ProviderRepository(tx).findById(id);
      if (row?.secretId != null) new SecretRepository(tx).removeUsage(row.secretId, "provider", id);
      tx.delete(provider).where(eq(provider.id, id)).run();
    });
  }

  setStatus(
    id: string,
    status: ProviderStatus,
    detail: string | null,
    latency: number | null,
    now = Date.now(),
  ): ProviderEntity | undefined {
    return this.update(id, {
      status,
      statusDetail: detail,
      lastLatencyMs: latency,
      lastProbeAt: now,
      updatedAt: now,
    });
  }

  setDefaultModel(
    id: string,
    modelId: string | null,
    now = Date.now(),
  ): ProviderEntity | undefined {
    return this.db.transaction((tx) => {
      if (new ProviderRepository(tx).findById(id) === undefined) return undefined;
      if (
        modelId !== null &&
        tx
          .select({ id: model.id })
          .from(model)
          .where(and(eq(model.id, modelId), eq(model.providerId, id)))
          .get() === undefined
      ) {
        throw new AppError(AppErrorCode.CONFLICT, "Модель не принадлежит провайдеру");
      }
      return tx
        .update(provider)
        .set({ defaultModelId: modelId, updatedAt: now })
        .where(eq(provider.id, id))
        .returning()
        .get();
    });
  }
}

function requireAuthModeIntegrity(row: ProviderEntity): void {
  if (row.authMode === "account") {
    if (row.secretId !== null) {
      throw integrityError(row, "Провайдер в режиме аккаунта не может ссылаться на секрет");
    }
    if (row.accountId === null && row.status !== "needs-relink") {
      throw integrityError(row, "Провайдер в режиме аккаунта должен быть привязан к аккаунту");
    }
    return;
  }
  if (row.accountId !== null) {
    throw integrityError(row, "Провайдер в режиме API не может быть привязан к аккаунту");
  }
}

function integrityError(row: ProviderEntity, message: string): AppError {
  return new AppError(AppErrorCode.VALIDATION_FAILED, message, {
    details: {
      providerId: row.id,
      authMode: row.authMode,
      hasSecret: row.secretId !== null,
      hasAccount: row.accountId !== null,
    },
  });
}

function capabilityColumns(capabilities: ProviderCapability[]) {
  return {
    capabilities: [...new Set(capabilities)],
    capText: capabilities.includes("text"),
    capEmbedding: capabilities.includes("embedding"),
    capImage: capabilities.includes("image"),
  };
}
