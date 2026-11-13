import { makeAutoObservable, runInAction } from "mobx";
import { isIpcError, type IpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  SUMMARIZER_MODELS_MAX,
  type AssistantPreferencesDto,
  type Contract,
  type ProviderDto,
  type SummarizerModelDto,
} from "@zvs/shared";
import { ASSISTANT_SAVE_FAILED } from "./assistantCopy";
import { modelViews, type PreferredModelView } from "../preferences/preferredModels";

function sameModel(left: SummarizerModelDto, right: SummarizerModelDto): boolean {
  return left.providerId === right.providerId && left.modelId === right.modelId;
}

function saveError(error: unknown): string {
  if (isIpcError(error) && error.code === AppErrorCode.VALIDATION_FAILED && error.message !== "") {
    return error.message;
  }
  return ASSISTANT_SAVE_FAILED;
}

export default class AssistantPreferencesStore {
  autoCompact = true;
  recaps = true;
  models: SummarizerModelDto[] = [];
  providers: ProviderDto[] = [];
  draftProviderId: string | null = null;
  draftModelId: string | null = null;
  error: string | null = null;
  loading = false;
  loaded = false;
  private saving: Promise<void> = Promise.resolve();

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<AssistantPreferencesStore, "ipc" | "saving">(
      this,
      { ipc: false, saving: false },
      { autoBind: true },
    );
  }

  get preferences(): AssistantPreferencesDto {
    return {
      autoCompact: this.autoCompact,
      recaps: this.recaps,
      summarizerModels: this.models,
    };
  }

  get draftProvider(): ProviderDto | undefined {
    return this.providers.find((provider) => provider.id === this.draftProviderId);
  }

  get draft(): SummarizerModelDto | null {
    if (this.draftProviderId === null || this.draftModelId === null) return null;
    return { providerId: this.draftProviderId, modelId: this.draftModelId };
  }

  get canAdd(): boolean {
    const draft = this.draft;
    return (
      draft !== null &&
      this.models.length < SUMMARIZER_MODELS_MAX &&
      !this.models.some((model) => sameModel(model, draft))
    );
  }

  get full(): boolean {
    return this.models.length >= SUMMARIZER_MODELS_MAX;
  }

  get views(): PreferredModelView[] {
    return modelViews(this.models, this.providers);
  }

  async load(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    try {
      const [preferences, providers] = await Promise.all([
        this.ipc.call("assistant.get", undefined),
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

  setAutoCompact(autoCompact: boolean): Promise<void> {
    if (autoCompact === this.autoCompact) return this.saving;
    this.autoCompact = autoCompact;
    return this.save({ autoCompact });
  }

  setRecaps(recaps: boolean): Promise<void> {
    if (recaps === this.recaps) return this.saving;
    this.recaps = recaps;
    return this.save({ recaps });
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
    return this.save({ summarizerModels: this.models });
  }

  remove(index: number): Promise<void> {
    if (index < 0 || index >= this.models.length) return this.saving;
    this.models = this.models.filter((_, position) => position !== index);
    return this.save({ summarizerModels: this.models });
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
    return this.save({ summarizerModels: this.models });
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

  private apply(preferences: AssistantPreferencesDto): void {
    this.autoCompact = preferences.autoCompact;
    this.recaps = preferences.recaps;
    this.models = preferences.summarizerModels;
  }

  private save(change: Partial<AssistantPreferencesDto>): Promise<void> {
    this.saving = this.saving.then(async () => {
      try {
        const current = await this.ipc.call("assistant.get", undefined);
        const stored = await this.ipc.call("assistant.set", { ...current, ...change });
        runInAction(() => {
          this.error = null;
          this.apply(stored);
        });
      } catch (error) {
        const stored = await this.ipc.call("assistant.get", undefined).catch(() => null);
        runInAction(() => {
          this.error = saveError(error);
          if (stored !== null) this.apply(stored);
        });
      }
    });
    return this.saving;
  }
}
