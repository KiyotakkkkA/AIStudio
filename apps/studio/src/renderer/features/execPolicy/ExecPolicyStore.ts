import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract, ProviderDto } from "@zvs/shared";

export const EXEC_MODES = ["always-ask", "safety-ask", "auto-bypass", "no-restrictions"] as const;

export type ExecMode = (typeof EXEC_MODES)[number];

export default class ExecPolicyStore {
  mode: ExecMode = "always-ask";
  judgeProviderId: string | null = null;
  judgeModelId: string | null = null;
  providers: ProviderDto[] = [];
  loading = false;
  loaded = false;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<ExecPolicyStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  get judgeProvider(): ProviderDto | undefined {
    return this.providers.find((provider) => provider.id === this.judgeProviderId);
  }

  get judgeReady(): boolean {
    return this.judgeProviderId !== null && this.judgeModelId !== null;
  }

  isAvailable(mode: ExecMode): boolean {
    return mode !== "auto-bypass" || this.judgeReady;
  }

  setMode(mode: ExecMode): void {
    if (!this.isAvailable(mode)) return;
    this.mode = mode;
  }

  selectJudgeProvider(id: string): void {
    const provider = this.providers.find((item) => item.id === id);
    if (provider === undefined) return;
    this.judgeProviderId = provider.id;
    this.judgeModelId = provider.models.find((model) => model.available)?.externalId ?? null;
    this.dropUnavailableMode();
  }

  setJudgeModel(id: string): void {
    if (this.judgeProvider?.models.some((model) => model.externalId === id) !== true) return;
    this.judgeModelId = id;
  }

  clearJudge(): void {
    this.judgeProviderId = null;
    this.judgeModelId = null;
    this.dropUnavailableMode();
  }

  async load(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    try {
      const summaries = await this.ipc.call("providers.list", {
        capability: "text",
        enabled: true,
      });
      const providers = await Promise.all(
        summaries.map((provider) =>
          this.ipc.call("providers.get", { id: provider.id, selectedOnly: true }),
        ),
      );
      runInAction(() => {
        this.providers = providers;
        this.loaded = true;
        if (this.judgeProviderId !== null && this.judgeProvider === undefined) this.clearJudge();
      });
    } finally {
      runInAction(() => {
        this.loading = false;
      });
    }
  }

  private dropUnavailableMode(): void {
    if (!this.isAvailable(this.mode)) this.mode = "safety-ask";
  }
}
