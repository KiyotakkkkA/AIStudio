import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract, ProviderId, ScenarioParams } from "@zvs/shared";

export type EntityKind =
  "provider" | "model" | "vectorStore" | "conversation" | "mcpServer" | "integration";

export const ENTITY_PARAMS: Readonly<Record<string, EntityKind>> = {
  providerId: "provider",
  modelId: "model",
  model: "model",
  storeId: "vectorStore",
  vectorStoreId: "vectorStore",
  conversationId: "conversation",
  serverId: "mcpServer",
  accountId: "integration",
};

export const ENTITY_LABELS: Record<EntityKind, string> = {
  provider: "Провайдер",
  model: "Модель",
  vectorStore: "Хранилище",
  conversation: "Диалог",
  mcpServer: "MCP-сервер",
  integration: "Интеграция",
};

export const ENTITY_PLACEHOLDERS: Record<EntityKind, string> = {
  provider: "Выберите провайдера",
  model: "Выберите модель",
  vectorStore: "Выберите хранилище",
  conversation: "Выберите диалог",
  mcpServer: "Выберите сервер",
  integration: "Выберите интеграцию",
};

export const MODEL_NEEDS_PROVIDER = "Сначала выберите провайдера";
export const ENTITY_MISSING = "Не найден";

export interface EntityOption {
  readonly value: string;
  readonly label: string;
}

export function entityKindOf(param: string): EntityKind | null {
  return ENTITY_PARAMS[param] ?? null;
}

export function providerOf(params: ScenarioParams): string | null {
  const value = params.providerId;
  return typeof value === "string" && value.length > 0 ? value : null;
}

export function withParam(params: ScenarioParams, key: string, value: string): ScenarioParams {
  const next: ScenarioParams = { ...params };
  if (value.length === 0) delete next[key];
  else next[key] = value;
  if (key === "providerId" && value !== params.providerId) {
    delete next.modelId;
    delete next.model;
  }
  return next;
}

export function withCurrent(
  options: readonly EntityOption[],
  current: string,
): readonly EntityOption[] {
  if (current.length === 0 || options.some((option) => option.value === current)) return options;
  return [{ value: current, label: `${current} · ${ENTITY_MISSING}` }, ...options];
}

type Listed = Exclude<EntityKind, "model">;

export default class EntityOptions {
  lists: Partial<Record<Listed, readonly EntityOption[]>> = {};
  models: Record<string, readonly EntityOption[]> = {};
  private readonly pending = new Set<string>();

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<EntityOptions, "ipc" | "pending">(
      this,
      { ipc: false, pending: false },
      { autoBind: true },
    );
  }

  options(kind: EntityKind, params: ScenarioParams): readonly EntityOption[] | null {
    if (kind === "model") {
      const provider = providerOf(params);
      if (provider === null) return null;
      void this.ensureModels(provider);
      return this.models[provider] ?? [];
    }
    void this.ensure(kind);
    return this.lists[kind] ?? [];
  }

  private async fetch(kind: Listed): Promise<EntityOption[]> {
    switch (kind) {
      case "provider":
        return (await this.ipc.call("providers.list", {})).map((entry) => ({
          value: entry.id,
          label: entry.name,
        }));
      case "vectorStore":
        return (await this.ipc.call("vectorStores.list", undefined)).map((entry) => ({
          value: entry.id,
          label: entry.name,
        }));
      case "conversation":
        return (await this.ipc.call("chat.conversations.list", undefined)).map((entry) => ({
          value: entry.id,
          label: entry.title,
        }));
      case "mcpServer":
        return (await this.ipc.call("mcp.list", undefined)).map((entry) => ({
          value: entry.id,
          label: entry.name,
        }));
      case "integration":
        return (await this.ipc.call("integrations.list", undefined)).map((entry) => ({
          value: entry.id,
          label: entry.name,
        }));
    }
  }

  async ensure(kind: Listed): Promise<void> {
    if (this.lists[kind] !== undefined || this.pending.has(kind)) return;
    this.pending.add(kind);
    try {
      const options = await this.fetch(kind);
      runInAction(() => {
        this.lists[kind] = options;
      });
    } catch {
      runInAction(() => {
        this.lists[kind] = [];
      });
    } finally {
      this.pending.delete(kind);
    }
  }

  async ensureModels(provider: string): Promise<void> {
    const key = `model:${provider}`;
    if (this.models[provider] !== undefined || this.pending.has(key)) return;
    this.pending.add(key);
    try {
      const detail = await this.ipc.call("providers.get", { id: provider as ProviderId });
      runInAction(() => {
        this.models[provider] = detail.models
          .filter((model) => model.available)
          .map((model) => ({
            value: model.externalId,
            label: model.displayName || model.externalId,
          }));
      });
    } catch {
      runInAction(() => {
        this.models[provider] = [];
      });
    } finally {
      this.pending.delete(key);
    }
  }
}
