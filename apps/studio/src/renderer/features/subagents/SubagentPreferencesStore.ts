import { makeAutoObservable, runInAction } from "mobx";
import { isIpcError, type IpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  SUBAGENT_CONCURRENCY_DEFAULT,
  SUBAGENT_CONCURRENCY_MAX,
  SUBAGENT_CONCURRENCY_MIN,
  SUBAGENT_MODELS_MAX,
  type Contract,
  type ProviderDto,
  type SubagentModelDto,
  type SubagentPreferencesDto,
} from "@zvs/shared";
import { SUBAGENT_SAVE_FAILED } from "./subagentCopy";

export interface SubagentModelView {
  readonly key: string;
  readonly providerName: string;
  readonly modelName: string;
  readonly account: boolean;
  readonly missing: boolean;
}

function sameModel(left: SubagentModelDto, right: SubagentModelDto): boolean {
  return left.providerId === right.providerId && left.modelId === right.modelId;
}

function saveError(error: unknown): string {
  if (isIpcError(error) && error.code === AppErrorCode.VALIDATION_FAILED && error.message !== "") {
    return error.message;
  }
  return SUBAGENT_SAVE_FAILED;
}

export default class SubagentPreferencesStore {
  enabled = true;
  models: SubagentModelDto[] = [];
  concurrency = SUBAGENT_CONCURRENCY_DEFAULT;
  providers: ProviderDto[] = [];
  draftProviderId: string | null = null;
  draftModelId: string | null = null;
  error: string | null = null;
  loading = false;
  loaded = false;
  private saving: Promise<void> = Promise.resolve();

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<SubagentPreferencesStore, "ipc" | "saving">(
      this,
      { ipc: false, saving: false },
      { autoBind: true },
    );
  }

  get preferences(): SubagentPreferencesDto {
    return { enabled: this.enabled, models: this.models, concurrency: this.concurrency };
  }

  get draftProvider(): ProviderDto | undefined {
    return this.providers.find((provider) => provider.id === this.draftProviderId);
  }

  get draft(): SubagentModelDto | null {
    if (this.draftProviderId === null || this.draftModelId === null) return null;
    return { providerId: this.draftProviderId, modelId: this.draftModelId };
  }

  get canAdd(): boolean {
    const draft = this.draft;
    return (
      draft !== null &&
      this.models.length < SUBAGENT_MODELS_MAX &&
      !this.models.some((model) => sameModel(model, draft))
    );
  }

  get full(): boolean {
    return this.models.length >= SUBAGENT_MODELS_MAX;
  }

  get views(): SubagentModelView[] {
    return this.models.map((entry) => {
      const provider = this.providers.find((item) => item.id === entry.providerId);
      const model = provider?.models.find(
        (item) => item.externalId === entry.modelId || item.id === entry.modelId,
      );
      return {
        key: `${entry.providerId}/${entry.modelId}`,
        providerName: provider?.name ?? entry.providerId,
        modelName: model === undefined ? entry.modelId : model.displayName || model.externalId,
        account: provider?.authMode === "account",
        missing: model === undefined || !model.available,
      };
    });
  }

  get accountListed(): boolean {
    return this.views.some((view) => view.account);
  }

  async load(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    try {
      const [preferences, providers] = await Promise.all([
        this.ipc.call("subagents.get", undefined),
        this.loadProviders(),
      ]);
      runInAction(() => {
        this.providers = providers;
        this.apply(preferences);
        this.loaded = true;
      });
    } finally {
      runInAction(() => {
        this.loading = false;
      });
    }
  }

  setEnabled(enabled: boolean): Promise<void> {
    if (enabled === this.enabled) return this.saving;
    this.enabled = enabled;
    return this.save({ enabled });
  }

  setConcurrency(value: number): Promise<void> {
    const concurrency = Math.min(
      SUBAGENT_CONCURRENCY_MAX,
      Math.max(SUBAGENT_CONCURRENCY_MIN, Math.round(value)),
    );
    if (!Number.isFinite(concurrency) || concurrency === this.concurrency) return this.saving;
    this.concurrency = concurrency;
    return this.save({ concurrency });
  }

  selectDraftProvider(id: string): void {
    const provider = this.providers.find((item) => item.id === id);
    if (provider === undefined) return;
    this.draftProviderId = provider.id;
    this.draftModelId = provider.models.find((model) => model.available)?.externalId ?? null;
  }

  setDraftModel(id: string): void {
    if (this.draftProvider?.models.some((model) => model.externalId === id) !== true) return;
    this.draftModelId = id;
  }

  addDraft(): Promise<void> {
    const draft = this.draft;
    if (draft === null || !this.canAdd) return this.saving;
    this.models = [...this.models, draft];
    this.draftProviderId = null;
    this.draftModelId = null;
    return this.save({ models: this.models });
  }

  remove(index: number): Promise<void> {
    if (index < 0 || index >= this.models.length) return this.saving;
    this.models = this.models.filter((_, position) => position !== index);
    return this.save({ models: this.models });
  }

  move(index: number, offset: -1 | 1): Promise<void> {
    const target = index + offset;
    if (index < 0 || target < 0 || index >= this.models.length || target >= this.models.length) {
      return this.saving;
    }
    const next = [...this.models];
    const [moved] = next.splice(index, 1);
    if (moved === undefined) return this.saving;
    next.splice(target, 0, moved);
    this.models = next;
    return this.save({ models: this.models });
  }

  private async loadProviders(): Promise<ProviderDto[]> {
    const summaries = await this.ipc.call("providers.list", {
      capability: "text",
      enabled: true,
    });
    return Promise.all(
      summaries.map((provider) =>
        this.ipc.call("providers.get", { id: provider.id, selectedOnly: true }),
      ),
    );
  }

  private apply(preferences: SubagentPreferencesDto): void {
    this.enabled = preferences.enabled;
    this.models = preferences.models;
    this.concurrency = preferences.concurrency;
  }

  private save(change: Partial<SubagentPreferencesDto>): Promise<void> {
    this.saving = this.saving.then(async () => {
      try {
        const current = await this.ipc.call("subagents.get", undefined);
        const stored = await this.ipc.call("subagents.set", { ...current, ...change });
        runInAction(() => {
          this.error = null;
          this.apply(stored);
        });
      } catch (error) {
        const stored = await this.ipc.call("subagents.get", undefined).catch(() => null);
        runInAction(() => {
          this.error = saveError(error);
          if (stored !== null) this.apply(stored);
        });
      }
    });
    return this.saving;
  }
}
