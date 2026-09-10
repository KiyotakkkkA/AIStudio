import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  ACCOUNT_FAMILIES,
  type AccountDto,
  type AccountFamily,
  type AccountId,
  type AdapterDescriptorDto,
  type Contract,
  type StreamId,
} from "@zvs/shared";
import type { EventRouter, RoutedEvent } from "../../app/EventRouter";
import { providerErrorCopy } from "./providerErrors";

export type LinkPhase = "waiting" | "success" | "timeout" | "cancelled";

export interface LinkingState {
  readonly adapter: AccountFamily;
  readonly phase: LinkPhase;
  readonly detail: string | null;
  readonly elapsedMs: number;
  readonly totalMs: number;
}

export interface AccountStoreEnvironment {
  readonly ipc: IpcClient<Contract>;
  readonly events: EventRouter;
}

export class AccountStore {
  accounts: AccountDto[] = [];
  families: AccountFamily[] = [];
  linking: LinkingState | null = null;
  loading = false;
  loaded = false;
  unlinkingId: AccountId | null = null;
  refreshingId: AccountId | null = null;
  error: string | null = null;

  private readonly ipc: IpcClient<Contract>;
  private readonly events: EventRouter;
  private stopObserving: (() => void) | null = null;
  private linkStream: StreamId | null = null;

  constructor(environment: AccountStoreEnvironment) {
    this.ipc = environment.ipc;
    this.events = environment.events;
    makeAutoObservable<AccountStore, "ipc" | "events" | "stopObserving" | "linkStream">(
      this,
      { ipc: false, events: false, stopObserving: false, linkStream: false },
      { autoBind: true },
    );
  }

  get isEmpty(): boolean {
    return this.loaded && this.accounts.length === 0;
  }

  get busy(): boolean {
    return this.linking?.phase === "waiting";
  }

  accountFor(adapter: AccountFamily): AccountDto | null {
    return this.accounts.find((account) => account.adapter === adapter) ?? null;
  }

  async load(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    this.error = null;
    try {
      const families =
        this.families.length > 0
          ? this.families
          : accountFamilies(await this.ipc.call("providers.adapters", undefined));
      const accounts = await this.ipc.call("accounts.list", undefined);
      runInAction(() => {
        this.families = families;
        this.accounts = [...accounts];
        this.loaded = true;
        this.loading = false;
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = providerErrorCopy(error);
        this.loaded = true;
        this.loading = false;
      });
    }
  }

  async link(adapter: AccountFamily): Promise<void> {
    if (this.busy) return;
    this.error = null;
    this.linkStream = null;
    this.linking = { adapter, phase: "waiting", detail: null, elapsedMs: 0, totalMs: 0 };
    this.watch();
    try {
      const result = await this.ipc.call("accounts.link", { adapter });
      const linked = result.status === "linked";
      runInAction(() => {
        this.unwatch();
        if (this.linking?.adapter !== adapter) return;
        this.linking = {
          adapter,
          phase: linked ? "success" : result.status,
          detail: result.detail,
          elapsedMs: 0,
          totalMs: 0,
        };
        if (result.status === "linked") this.mergeAccount(result.account);
      });
      if (linked) await this.reload();
    } catch (error: unknown) {
      runInAction(() => {
        this.unwatch();
        if (this.linking?.adapter === adapter) this.linking = null;
        this.error = providerErrorCopy(error);
      });
    }
  }

  async cancelLink(): Promise<void> {
    const linking = this.linking;
    if (linking === null || linking.phase !== "waiting") return;
    try {
      await this.ipc.call("accounts.cancelLink", { adapter: linking.adapter });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = providerErrorCopy(error);
      });
    }
  }

  dismissLinking(): void {
    this.unwatch();
    this.linking = null;
  }

  async unlink(id: AccountId): Promise<void> {
    if (this.unlinkingId !== null) return;
    this.unlinkingId = id;
    this.error = null;
    try {
      await this.ipc.call("accounts.unlink", { id });
      runInAction(() => {
        this.unlinkingId = null;
        this.accounts = this.accounts.filter((account) => account.id !== id);
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.unlinkingId = null;
        this.error = providerErrorCopy(error);
      });
    }
  }

  async refresh(id: AccountId): Promise<void> {
    if (this.refreshingId !== null) return;
    this.refreshingId = id;
    this.error = null;
    try {
      const account = await this.ipc.call("accounts.refresh", { id });
      runInAction(() => {
        this.refreshingId = null;
        this.mergeAccount(account);
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.refreshingId = null;
        this.error = providerErrorCopy(error);
      });
    }
  }

  dismissError(): void {
    this.error = null;
  }

  dispose(): void {
    this.unwatch();
  }

  private async reload(): Promise<void> {
    try {
      const accounts = await this.ipc.call("accounts.list", undefined);
      runInAction(() => {
        this.accounts = [...accounts];
      });
    } catch {
      // The merged account already reflects the link; the next load reconciles the rest.
    }
  }

  private watch(): void {
    this.unwatch();
    this.stopObserving = this.events.observe(this.onEvent);
  }

  private unwatch(): void {
    this.stopObserving?.();
    this.stopObserving = null;
    this.linkStream = null;
  }

  private onEvent(event: RoutedEvent): void {
    const linking = this.linking;
    if (linking === null || linking.phase !== "waiting") return;
    if (event.type === "step") {
      const step = event.step;
      if (step.domain !== "accounts" || step.adapter !== linking.adapter) return;
      this.linkStream = event.streamId;
      return;
    }
    if (event.type !== "progress" || event.streamId !== this.linkStream) return;
    this.linking = { ...linking, elapsedMs: event.done, totalMs: event.total };
  }

  private mergeAccount(account: AccountDto): void {
    const index = this.accounts.findIndex((entry) => entry.id === account.id);
    if (index === -1) {
      this.accounts = [
        account,
        ...this.accounts.filter((entry) => entry.adapter !== account.adapter),
      ];
      return;
    }
    this.accounts = this.accounts.map((entry, at) => (at === index ? account : entry));
  }
}

function accountFamilies(adapters: readonly AdapterDescriptorDto[]): AccountFamily[] {
  return adapters
    .filter((adapter) => adapter.implemented && adapter.authModes.includes("account"))
    .map((adapter) => adapter.family)
    .filter((family): family is AccountFamily =>
      (ACCOUNT_FAMILIES as readonly string[]).includes(family),
    );
}

export default AccountStore;
