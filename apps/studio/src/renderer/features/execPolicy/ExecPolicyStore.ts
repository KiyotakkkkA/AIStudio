import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  EXEC_MODES,
  type Contract,
  type ExecMode,
  type ExecPolicyDto,
  type ProviderDto,
} from "@zvs/shared";

export { EXEC_MODES, type ExecMode };

export default class ExecPolicyStore {
  mode: ExecMode = "always-ask";
  judgeProviderId: string | null = null;
  judgeModelId: string | null = null;
  providers: ProviderDto[] = [];
  loading = false;
  loaded = false;
  private saving: Promise<void> = Promise.resolve();

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<ExecPolicyStore, "ipc" | "saving">(
      this,
      { ipc: false, saving: false },
      { autoBind: true },
    );
  }

  get judgeProvider(): ProviderDto | undefined {
    return this.providers.find((provider) => provider.id === this.judgeProviderId);
  }

  get judgeReady(): boolean {
    return this.judgeProviderId !== null && this.judgeModelId !== null;
  }

  get policy(): ExecPolicyDto {
    return {
      mode: this.mode,
      judge:
        this.judgeProviderId !== null && this.judgeModelId !== null
          ? { providerId: this.judgeProviderId, modelId: this.judgeModelId }
          : null,
    };
  }

  isAvailable(mode: ExecMode): boolean {
    return mode !== "auto-bypass" || this.judgeReady;
  }

  setMode(mode: ExecMode): Promise<void> {
    if (!this.isAvailable(mode) || mode === this.mode) return this.saving;
    this.mode = mode;
    return this.save();
  }

  selectJudgeProvider(id: string): Promise<void> {
    const provider = this.providers.find((item) => item.id === id);
    if (provider === undefined) return this.saving;
    this.judgeProviderId = provider.id;
    this.judgeModelId = provider.models.find((model) => model.available)?.externalId ?? null;
    this.dropUnavailableMode();
    return this.save();
  }

  setJudgeModel(id: string): Promise<void> {
    if (this.judgeProvider?.models.some((model) => model.externalId === id) !== true) {
      return this.saving;
    }
    this.judgeModelId = id;
    return this.save();
  }

  clearJudge(): Promise<void> {
    this.judgeProviderId = null;
    this.judgeModelId = null;
    this.dropUnavailableMode();
    return this.save();
  }

  async load(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    try {
      const [policy, providers] = await Promise.all([
        this.ipc.call("execPolicy.get", undefined),
        this.loadProviders(),
      ]);
      runInAction(() => {
        this.providers = providers;
        this.apply(policy);
        this.loaded = true;
      });
    } finally {
      runInAction(() => {
        this.loading = false;
      });
    }
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

  private apply(policy: ExecPolicyDto): void {
    this.mode = policy.mode;
    this.judgeProviderId = policy.judge?.providerId ?? null;
    this.judgeModelId = policy.judge?.modelId ?? null;
  }

  private save(): Promise<void> {
    const next = this.policy;
    this.saving = this.saving.then(async () => {
      try {
        const stored = await this.ipc.call("execPolicy.set", next);
        runInAction(() => this.apply(stored));
      } catch {
        const stored = await this.ipc.call("execPolicy.get", undefined).catch(() => null);
        if (stored !== null) runInAction(() => this.apply(stored));
      }
    });
    return this.saving;
  }

  private dropUnavailableMode(): void {
    if (!this.isAvailable(this.mode)) this.mode = "safety-ask";
  }
}
