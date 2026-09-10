import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  PROVIDER_CAPABILITIES,
  type AccountDto,
  type AdapterDescriptorDto,
  type Contract,
  type ModelDto,
  type ModelId,
  type ProbeResultDto,
  type ProviderCapability,
  type ProviderDto,
  type ProviderId,
  type ProviderSummaryDto,
  type SecretSummaryDto,
} from "@zvs/shared";
import { providerErrorCopy, providerFieldErrors } from "./providerErrors";
import { discoveredRows, modelRows, type ModelRow } from "./modelRows";
import ProviderFormVm from "./ProviderFormVm";
import { ACCOUNTS_TAB, type ProvidersTab } from "./providerTabs";

export type PendingIntent =
  | { readonly kind: "create" }
  | { readonly kind: "select"; readonly id: ProviderId }
  | { readonly kind: "capability"; readonly capability: ProviderCapability };

export class ProviderStore {
  capability: ProviderCapability = "text";
  /** The visible tab. `accounts` is a sibling of the capability filters, never one of them. */
  tab: ProvidersTab = "text";
  summaries: ProviderSummaryDto[] = [];
  counts: Record<ProviderCapability, number> = { text: 0, embedding: 0, image: 0 };
  selectedId: ProviderId | null = null;
  detail: ProviderDto | null = null;
  form: ProviderFormVm | null = null;
  adapters: AdapterDescriptorDto[] = [];
  secrets: SecretSummaryDto[] = [];
  accounts: AccountDto[] = [];
  pending: PendingIntent | null = null;
  modelQuery = "";
  loading = false;
  loaded = false;
  saving = false;
  removing = false;
  probing = false;
  refreshing = false;
  error: string | null = null;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<ProviderStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  get models(): ModelDto[] {
    return this.detail?.models ?? [];
  }

  get probeResult(): ProbeResultDto | null {
    return this.form?.probeResult ?? null;
  }

  get rows(): ModelRow[] {
    if (this.models.length > 0) return modelRows(this.models);
    const outcome = this.probeResult?.outcome;
    return outcome?.kind === "ok" ? discoveredRows(outcome.models) : [];
  }

  get visibleRows(): ModelRow[] {
    const query = this.modelQuery.trim().toLowerCase();
    if (query.length === 0) return this.rows;
    return this.rows.filter(
      (row) =>
        row.externalId.toLowerCase().includes(query) ||
        row.displayName.toLowerCase().includes(query) ||
        (row.family ?? "").toLowerCase().includes(query),
    );
  }

  get discoveredModelCount(): number {
    return this.summaries.reduce((total, summary) => total + summary.modelCount, 0);
  }

  get isEmpty(): boolean {
    return this.loaded && this.summaries.length === 0;
  }

  get capabilitySupported(): boolean {
    if (this.capability === "text") return true;
    return this.adapters.some(
      (adapter) =>
        adapter.implemented &&
        (this.capability === "embedding" ? adapter.embedding : adapter.image),
    );
  }

  get selectedSummary(): ProviderSummaryDto | null {
    return this.summaries.find((summary) => summary.id === this.selectedId) ?? null;
  }

  async load(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    this.error = null;
    try {
      const adapters = this.adapters.length > 0 ? this.adapters : await this.fetchAdapters();
      const [secrets, accounts] = await Promise.all([
        this.ipc.call("secrets.list", {}),
        this.ipc.call("accounts.list", undefined),
      ]);
      const lists = await Promise.all(
        PROVIDER_CAPABILITIES.map(async (capability) =>
          this.ipc.call("providers.list", { capability }),
        ),
      );
      runInAction(() => {
        this.adapters = [...adapters];
        this.secrets = [...secrets];
        this.accounts = [...accounts];
        this.applyLists(lists);
        this.loaded = true;
        this.loading = false;
      });
      await this.restoreSelection();
    } catch (error: unknown) {
      runInAction(() => {
        this.error = providerErrorCopy(error);
        this.loaded = true;
        this.loading = false;
      });
    }
  }

  async setCapability(capability: ProviderCapability): Promise<void> {
    this.tab = capability;
    if (capability === this.capability) return;
    this.capability = capability;
    this.selectedId = null;
    this.detail = null;
    this.form = null;
    await this.load();
  }

  requestCapability(capability: ProviderCapability): void {
    if (capability === this.capability && this.tab === capability) return;
    if (capability !== this.capability && this.form?.dirty === true) {
      this.pending = { kind: "capability", capability };
      return;
    }
    void this.setCapability(capability);
  }

  /**
   * Switching to Accounts keeps the form and its edits in the store, so it needs no
   * unsaved-changes guard — only the capability filters do.
   */
  requestTab(tab: ProvidersTab): void {
    if (tab === this.tab) return;
    if (tab === ACCOUNTS_TAB) {
      this.tab = tab;
      return;
    }
    this.requestCapability(tab);
  }

  async select(id: ProviderId): Promise<void> {
    this.selectedId = id;
    this.detail = null;
    this.form = null;
    try {
      const provider = await this.ipc.call("providers.get", { id });
      runInAction(() => {
        if (this.selectedId !== id) return;
        this.applyDetail(provider);
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = providerErrorCopy(error);
        if (this.selectedId === id) this.selectedId = null;
      });
    }
  }

  requestSelect(id: ProviderId): void {
    if (id === this.selectedId) return;
    if (this.form?.dirty === true) {
      this.pending = { kind: "select", id };
      return;
    }
    void this.select(id);
  }

  startCreate(): void {
    this.selectedId = null;
    this.detail = null;
    this.form = this.newForm(null);
  }

  requestCreate(): void {
    if (this.form?.dirty === true) {
      this.pending = { kind: "create" };
      return;
    }
    this.startCreate();
  }

  confirmPending(): void {
    const intent = this.pending;
    this.pending = null;
    if (intent === null) return;
    if (intent.kind === "create") this.startCreate();
    else if (intent.kind === "select") void this.select(intent.id);
    else void this.setCapability(intent.capability);
  }

  cancelPending(): void {
    this.pending = null;
  }

  resetForm(): void {
    this.form = this.detail === null ? null : this.newForm(this.detail);
  }

  setModelQuery(query: string): void {
    this.modelQuery = query;
  }

  dismissError(): void {
    this.error = null;
  }

  async submit(): Promise<boolean> {
    const form = this.form;
    if (form === null || this.saving) return false;
    form.setBanner(null);
    if (!form.validate()) return false;
    this.saving = true;
    this.error = null;
    try {
      const provider = form.isNew
        ? await this.ipc.call("providers.create", form.toCreateInput())
        : await this.ipc.call("providers.update", form.toUpdateInput());
      const probeResult = form.probeResult;
      runInAction(() => {
        this.saving = false;
        this.applyDetail(provider);
        if (probeResult !== null) this.form?.setProbeResult(probeResult);
        this.mergeSummary(provider);
      });
      await this.refreshCounts();
      return true;
    } catch (error: unknown) {
      const fieldErrors = providerFieldErrors(error);
      const hasFieldErrors = Object.keys(fieldErrors).length > 0;
      runInAction(() => {
        this.saving = false;
        if (this.form === null) {
          this.error = providerErrorCopy(error);
          return;
        }
        this.form.setErrors(fieldErrors);
        this.form.setBanner(hasFieldErrors ? null : providerErrorCopy(error));
      });
      return false;
    }
  }

  async remove(id: ProviderId): Promise<boolean> {
    if (this.removing) return false;
    this.removing = true;
    this.error = null;
    try {
      await this.ipc.call("providers.remove", { id });
      runInAction(() => {
        this.removing = false;
        this.summaries = this.summaries.filter((summary) => summary.id !== id);
        if (this.selectedId === id) {
          this.selectedId = null;
          this.detail = null;
          this.form = null;
        }
      });
      await this.refreshCounts();
      return true;
    } catch (error: unknown) {
      runInAction(() => {
        this.removing = false;
        this.error = providerErrorCopy(error);
      });
      return false;
    }
  }

  async removeSelected(): Promise<boolean> {
    return this.selectedId === null ? false : await this.remove(this.selectedId);
  }

  async probe(): Promise<void> {
    const form = this.form;
    if (form === null || this.probing) return;
    if (!form.validate()) return;
    this.probing = true;
    this.error = null;
    const savedAndClean = form.providerId !== null && !form.dirty;
    try {
      const result = await this.ipc.call(
        "providers.probe",
        savedAndClean && form.providerId !== null
          ? { id: form.providerId }
          : { draft: form.toConnectionInput() },
      );
      runInAction(() => {
        this.probing = false;
        this.form?.setProbeResult(result);
        if (result.provider !== null) {
          this.applyDetail(result.provider, { keepForm: true });
          this.mergeSummary(result.provider);
        }
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.probing = false;
        this.form?.forgetProbe();
        this.form?.setBanner(providerErrorCopy(error));
      });
    }
  }

  async setDefaultModel(modelId: ModelId | null): Promise<void> {
    if (this.selectedId === null) return;
    try {
      const provider = await this.ipc.call("providers.setDefaultModel", {
        id: this.selectedId,
        modelId,
      });
      runInAction(() => {
        this.applyDetail(provider, { keepForm: true });
        this.mergeSummary(provider);
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = providerErrorCopy(error);
      });
    }
  }

  async refreshAll(): Promise<void> {
    if (this.refreshing) return;
    this.refreshing = true;
    this.error = null;
    try {
      await this.ipc.call("providers.refreshAll", undefined);
      const lists = await Promise.all(
        PROVIDER_CAPABILITIES.map(async (capability) =>
          this.ipc.call("providers.list", { capability }),
        ),
      );
      runInAction(() => {
        this.refreshing = false;
        this.applyLists(lists);
      });
      if (this.selectedId !== null) await this.reloadDetail(this.selectedId);
    } catch (error: unknown) {
      runInAction(() => {
        this.refreshing = false;
        this.error = providerErrorCopy(error);
      });
    }
  }

  private async fetchAdapters(): Promise<AdapterDescriptorDto[]> {
    return [...(await this.ipc.call("providers.adapters", undefined))];
  }

  private async refreshCounts(): Promise<void> {
    const lists = await Promise.all(
      PROVIDER_CAPABILITIES.map(async (capability) =>
        this.ipc.call("providers.list", { capability }),
      ),
    );
    runInAction(() => {
      this.applyLists(lists);
    });
  }

  private async reloadDetail(id: ProviderId): Promise<void> {
    try {
      const provider = await this.ipc.call("providers.get", { id });
      runInAction(() => {
        this.applyDetail(provider, { keepForm: this.form?.dirty === true });
      });
    } catch {
      // The list stays authoritative; a stale detail is replaced on the next selection.
    }
  }

  private async restoreSelection(): Promise<void> {
    if (this.selectedId !== null) return;
    const first = this.summaries[0];
    if (first === undefined) return;
    await this.select(first.id);
  }

  private applyLists(lists: readonly (readonly ProviderSummaryDto[])[]): void {
    const counts: Record<ProviderCapability, number> = { text: 0, embedding: 0, image: 0 };
    PROVIDER_CAPABILITIES.forEach((capability, index) => {
      counts[capability] = lists[index]?.length ?? 0;
    });
    this.counts = counts;
    const active = lists[PROVIDER_CAPABILITIES.indexOf(this.capability)] ?? [];
    this.summaries = [...active];
  }

  private applyDetail(provider: ProviderDto, options?: { readonly keepForm?: boolean }): void {
    this.detail = provider;
    this.selectedId = provider.id;
    if (options?.keepForm === true && this.form !== null) return;
    this.form = this.newForm(provider);
  }

  private mergeSummary(provider: ProviderDto): void {
    const summary: ProviderSummaryDto = {
      id: provider.id,
      kind: provider.kind,
      adapter: provider.adapter,
      authMode: provider.authMode,
      name: provider.name,
      capabilities: [...provider.capabilities],
      enabled: provider.enabled,
      status: provider.status,
      statusDetail: provider.statusDetail,
      lastProbeAt: provider.lastProbeAt,
      lastLatencyMs: provider.lastLatencyMs,
      modelCount: provider.models.length,
      defaultModelId: provider.defaultModelId,
      updatedAt: provider.updatedAt,
    };
    const index = this.summaries.findIndex((entry) => entry.id === provider.id);
    if (index === -1) {
      if (provider.capabilities.includes(this.capability)) {
        this.summaries = [summary, ...this.summaries];
      }
      return;
    }
    this.summaries = this.summaries.map((entry, at) => (at === index ? summary : entry));
  }

  private newForm(provider: ProviderDto | null): ProviderFormVm {
    return new ProviderFormVm({
      adapters: this.adapters,
      accounts: this.accounts,
      capability: this.capability,
      provider,
    });
  }
}

export default ProviderStore;
