import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type {
  ChatEvalRunDto,
  Contract,
  ProviderId,
  RetrievalMode,
  VectorStoreId,
} from "@zvs/shared";

export interface ChatEvalModel {
  readonly key: string;
  readonly providerId: ProviderId;
  readonly modelId: string;
  readonly label: string;
}

export const CHAT_EVAL_MODES: readonly RetrievalMode[] = ["auto", "agentic"];

export const CHAT_EVAL_MODE_LABELS: Record<RetrievalMode, string> = {
  auto: "Перед ответом",
  hybrid: "Гибрид",
  agentic: "Решает модель",
};

export default class ChatEvalStore {
  models: ChatEvalModel[] = [];
  modelKey = "";
  run: ChatEvalRunDto | null = null;
  loaded = false;
  running = false;
  error: string | null = null;
  private storeId: VectorStoreId | null = null;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<ChatEvalStore, "ipc" | "storeId">(
      this,
      { ipc: false, storeId: false },
      { autoBind: true },
    );
  }

  get selected(): ChatEvalModel | undefined {
    return this.models.find((model) => model.key === this.modelKey);
  }

  canRun(cases: number): boolean {
    return !this.running && this.storeId !== null && this.selected !== undefined && cases > 0;
  }

  setModel(key: string) {
    this.modelKey = key;
  }

  reset(storeId: VectorStoreId | null) {
    this.storeId = storeId;
    this.run = null;
    this.error = null;
    this.running = false;
  }

  async loadModels() {
    if (this.loaded) return;
    try {
      const providers = await this.ipc.call("providers.list", { capability: "text" });
      const entries = await Promise.all(
        providers
          .filter((provider) => provider.enabled && provider.capabilities.includes("text"))
          .map(async (provider) => {
            const detail = await this.ipc.call("providers.get", {
              id: provider.id,
              selectedOnly: true,
            });
            return detail.models
              .filter((model) => model.available)
              .map((model) => ({
                key: `${provider.id}|${model.externalId}`,
                providerId: provider.id,
                modelId: model.externalId,
                label: `${provider.name} · ${model.displayName || model.externalId}`,
              }));
          }),
      );
      runInAction(() => {
        this.models = entries.flat();
        this.loaded = true;
        if (this.selected === undefined) this.modelKey = this.models[0]?.key ?? "";
      });
    } catch (error) {
      runInAction(() => {
        this.error = errorCopy(error);
      });
    }
  }

  async evaluate() {
    const storeId = this.storeId;
    const model = this.selected;
    if (storeId === null || model === undefined || this.running) return;
    this.running = true;
    this.error = null;
    try {
      const run = await this.ipc.call("chat.evaluate", {
        storeId,
        providerId: model.providerId,
        modelId: model.modelId,
        modes: [...CHAT_EVAL_MODES],
      });
      runInAction(() => {
        if (this.storeId === storeId) this.run = run;
      });
    } catch (error) {
      runInAction(() => {
        if (this.storeId === storeId) this.error = errorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.running = false;
      });
    }
  }
}

function errorCopy(error: unknown) {
  return error instanceof Error
    ? error.message
    : "Не удалось выполнить операцию. Попробуйте ещё раз.";
}
