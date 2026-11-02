import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type {
  Contract,
  IntegrationAccountDetailDto,
  IntegrationAccountDto,
  IntegrationActivityDto,
  IntegrationId,
  IntegrationKind,
  IntegrationKindDto,
  IntegrationTestResultDto,
  ScenarioDto,
  SecretSummaryDto,
  WorkerDto,
} from "@zvs/shared";
import type { EventRouter, RoutedEvent } from "../../app/EventRouter";
import {
  ACCOUNT_STATE_LABELS,
  ACCOUNT_STATE_TONES,
  PLUGIN_KINDS,
  PLUGIN_SECTIONS,
  defaultAccountName,
  formatWait,
  matchesQuery,
  type AccountState,
  type CatalogueTab,
  type PluginSection,
} from "./pluginPresentation";
import { PluginFormVm, secretOptions, type SecretOption } from "./PluginFormVm";
import { pluginErrorCopy, pluginFieldErrors, isSessionExpired } from "./pluginErrors";
import { accountPath, pluginPath } from "./pluginRoutes";
import {
  SIGN_IN_TYPES,
  isSignedIn,
  signInTypeOf,
  type SignInType,
  type SignInTypeId,
} from "./signInTypes";
import type { StatusTone } from "../../ui/atoms/statusTone";

export const INTEGRATIONS_DOMAIN = "integrations";
export const DEVICE_CODE_PHASE = "device-code";
export const ACTIVITY_LIMIT = 50;
const WORKER_PREFIX = "integration:";

export interface SignInState {
  readonly accountId: IntegrationId;
  readonly userCode: string | null;
  readonly verificationUri: string | null;
  readonly expiresAt: number | null;
}

export interface CatalogueEntry {
  readonly kind: IntegrationKind;
  readonly count: number;
}

export interface CatalogueSection extends PluginSection {
  readonly entries: readonly CatalogueEntry[];
}

export interface PluginStoreOptions {
  readonly clock?: () => number;
}

export default class PluginStore {
  kinds: IntegrationKindDto[] = [];
  accounts: IntegrationAccountDto[] = [];
  secrets: SecretSummaryDto[] = [];
  scenarios: ScenarioDto[] = [];
  workers: WorkerDto[] = [];
  loaded = false;
  loading = false;
  error: string | null = null;

  query = "";
  tab: CatalogueTab = "all";
  collapsed: string[] = [];

  kind: IntegrationKind | null = null;
  accountId: IntegrationId | null = null;
  adding = false;
  draftType: SignInTypeId | null = null;
  form: PluginFormVm | null = null;
  detail: IntegrationAccountDetailDto | null = null;
  activity: IntegrationActivityDto[] = [];
  tests: Record<string, IntegrationTestResultDto> = {};
  signIn: SignInState | null = null;
  expired: string[] = [];
  saving = false;
  testing = false;
  toggling = false;
  removing = false;
  redirect: string | null = null;

  readonly now: () => number;

  private readonly ipc: IpcClient<Contract>;
  private readonly events: EventRouter;
  private stopObserving: (() => void) | null = null;
  private cancelling = false;
  private enableAfterSignIn = new Set<string>();

  constructor(ipc: IpcClient<Contract>, events: EventRouter, options: PluginStoreOptions = {}) {
    this.ipc = ipc;
    this.events = events;
    this.now = options.clock ?? Date.now;
    makeAutoObservable<
      PluginStore,
      "ipc" | "events" | "stopObserving" | "cancelling" | "enableAfterSignIn"
    >(
      this,
      {
        ipc: false,
        events: false,
        stopObserving: false,
        cancelling: false,
        enableAfterSignIn: false,
        now: false,
      },
      { autoBind: true },
    );
  }

  get availableKinds(): IntegrationKind[] {
    if (this.kinds.length === 0) return [...PLUGIN_KINDS];
    const known = new Set(this.kinds.map((entry) => entry.kind));
    return PLUGIN_KINDS.filter((kind) => known.has(kind));
  }

  get sections(): CatalogueSection[] {
    const available = new Set(this.availableKinds);
    return PLUGIN_SECTIONS.map((section) => ({
      ...section,
      entries: section.kinds
        .filter((kind) => available.has(kind))
        .filter((kind) => matchesQuery(kind, this.query))
        .map((kind) => ({ kind, count: this.accountsOf(kind).length }))
        .filter((entry) => this.tab === "all" || entry.count > 0),
    })).filter((section) => section.entries.length > 0);
  }

  get catalogueEmpty(): boolean {
    return this.sections.length === 0;
  }

  get selectedAccount(): IntegrationAccountDto | null {
    if (this.accountId === null) return null;
    return this.accounts.find((account) => account.id === this.accountId) ?? null;
  }

  get kindDescriptor(): IntegrationKindDto | null {
    if (this.kind === null) return null;
    return this.kinds.find((entry) => entry.kind === this.kind) ?? null;
  }

  get signInTypes(): readonly SignInType[] {
    return this.kind === null ? [] : SIGN_IN_TYPES[this.kind];
  }

  get secretChoices(): SecretOption[] {
    return secretOptions(this.secrets);
  }

  get usedBy(): ScenarioDto[] {
    const id = this.accountId;
    if (id === null) return [];
    return this.scenarios.filter((scenario) => scenario.trigger?.params.accountId === id);
  }

  get selectedTest(): IntegrationTestResultDto | null {
    return this.accountId === null ? null : (this.tests[this.accountId] ?? null);
  }

  get signingIn(): boolean {
    return this.signIn !== null;
  }

  accountsOf(kind: IntegrationKind): IntegrationAccountDto[] {
    return this.accounts
      .filter((account) => account.kind === kind)
      .sort((left, right) => left.createdAt - right.createdAt);
  }

  workerOf(accountId: string): WorkerDto | null {
    return this.workers.find((worker) => worker.key === `${WORKER_PREFIX}${accountId}`) ?? null;
  }

  stateOf(account: IntegrationAccountDto): AccountState {
    if (this.signIn?.accountId === account.id) return "signing";
    if (!account.enabled || account.status === "disabled") return "disabled";
    if (this.expired.includes(account.id)) return "needsSignIn";
    if (signInTypeOf(account).browser && !isSignedIn(account.config)) return "needsSignIn";
    return account.status;
  }

  stateLabel(account: IntegrationAccountDto): string {
    return ACCOUNT_STATE_LABELS[this.stateOf(account)];
  }

  stateTone(account: IntegrationAccountDto): StatusTone {
    return ACCOUNT_STATE_TONES[this.stateOf(account)];
  }

  healthLine(account: IntegrationAccountDto): string | null {
    const worker = this.workerOf(account.id);
    const detail = account.statusDetail ?? worker?.detail ?? null;
    const retryAt = worker?.retryAt ?? null;
    const wait = retryAt === null ? null : retryAt - this.now();
    const waiting = wait !== null && wait > 0 ? `Повтор через ${formatWait(wait)}` : null;
    const parts = [detail, waiting].filter((part): part is string => part !== null);
    return parts.length === 0 ? null : parts.join(" · ");
  }

  mount(): Promise<void> {
    if (this.stopObserving === null) this.stopObserving = this.events.observe(this.onEvent);
    return this.load();
  }

  dispose(): void {
    this.stopObserving?.();
    this.stopObserving = null;
  }

  async load(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    this.error = null;
    try {
      const [kinds, accounts, secrets, scenarios, workers] = await Promise.all([
        this.kinds.length > 0
          ? Promise.resolve(this.kinds)
          : this.ipc.call("integrations.kinds", undefined),
        this.ipc.call("integrations.list", undefined),
        this.ipc.call("secrets.list", {}).catch((): SecretSummaryDto[] => []),
        this.ipc.call("scenarios.list", undefined).catch((): ScenarioDto[] => []),
        this.ipc.call("workers.list", undefined).catch((): WorkerDto[] => []),
      ]);
      runInAction(() => {
        this.kinds = [...kinds];
        this.accounts = [...accounts];
        this.secrets = [...secrets];
        this.scenarios = [...scenarios];
        this.workers = [...workers];
        this.loaded = true;
        this.loading = false;
        this.refreshSelection();
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = pluginErrorCopy(error);
        this.loaded = true;
        this.loading = false;
      });
    }
  }

  setQuery(query: string): void {
    this.query = query;
  }

  setTab(tab: CatalogueTab): void {
    this.tab = tab;
  }

  toggleSection(id: string): void {
    this.collapsed = this.collapsed.includes(id)
      ? this.collapsed.filter((entry) => entry !== id)
      : [...this.collapsed, id];
  }

  isCollapsed(id: string): boolean {
    return this.collapsed.includes(id) && this.query.trim().length === 0;
  }

  select(kind: IntegrationKind | null, accountId: string | null): void {
    if (kind !== this.kind) {
      this.adding = false;
      this.draftType = null;
      this.form = null;
    }
    this.kind = kind;
    this.accountId = kind === null ? null : (accountId as IntegrationId | null);
    this.refreshSelection();
  }

  consumeRedirect(): string | null {
    const target = this.redirect;
    this.redirect = null;
    return target;
  }

  startAdding(kind?: IntegrationKind): void {
    const target = kind ?? this.kind;
    if (target === null) return;
    if (target !== this.kind) this.select(target, null);
    this.adding = true;
    this.accountId = null;
    this.detail = null;
    this.activity = [];
    this.draftType = null;
    this.form = null;
    this.redirect = pluginPath(target);
    this.refreshSelection();
  }

  cancelAdding(): void {
    this.adding = false;
    this.draftType = null;
    this.form = null;
    this.refreshSelection();
  }

  chooseType(id: SignInTypeId): void {
    const kind = this.kind;
    if (kind === null) return;
    const type = SIGN_IN_TYPES[kind].find((entry) => entry.id === id);
    if (type === undefined) return;
    this.draftType = id;
    this.form = new PluginFormVm(
      this.kindDescriptor?.fields ?? [],
      type,
      null,
      defaultAccountName(kind, this.accounts),
    );
  }

  async save(): Promise<IntegrationId | null> {
    const form = this.form;
    if (form === null || this.saving || !form.validate()) return null;
    this.saving = true;
    this.error = null;
    try {
      const name = form.name.trim();
      const config = form.config();
      const saved =
        form.accountId === null
          ? await this.ipc.call("integrations.create", {
              kind: form.type.kind,
              name,
              config,
              enabled: !form.type.browser,
            })
          : await this.ipc.call("integrations.update", { id: form.accountId, name, config });
      runInAction(() => {
        this.saving = false;
        this.merge(saved);
        if (form.isNew) {
          if (form.type.browser) this.enableAfterSignIn.add(saved.id);
          this.adding = false;
          this.draftType = null;
          this.redirect = accountPath(saved.kind, saved.id);
        }
        this.form = this.editForm(saved);
        this.accountId = saved.id;
      });
      await this.reloadAccount(saved.id);
      return saved.id;
    } catch (error: unknown) {
      runInAction(() => {
        this.saving = false;
        const fields = pluginFieldErrors(error);
        if (Object.keys(fields).length > 0) form.setErrors(fields);
        else this.error = pluginErrorCopy(error);
      });
      return null;
    }
  }

  async startSignIn(): Promise<void> {
    if (this.signIn !== null) return;
    const form = this.form;
    if (form !== null && !form.validate()) return;
    let id = this.accountId;
    if (form !== null && (form.isNew || form.dirty)) id = await this.save();
    if (id === null) return;
    this.cancelling = false;
    this.error = null;
    this.signIn = { accountId: id, userCode: null, verificationUri: null, expiresAt: null };
    this.expired = this.expired.filter((entry) => entry !== id);
    try {
      const account = await this.ipc.call("integrations.signIn", { id });
      runInAction(() => {
        this.merge(account);
        this.signIn = null;
      });
      if (this.enableAfterSignIn.delete(id) && !account.enabled) {
        const enabled = await this.ipc.call("integrations.enable", { id });
        runInAction(() => {
          this.merge(enabled);
        });
      }
    } catch (error: unknown) {
      runInAction(() => {
        this.signIn = null;
        if (this.cancelling) return;
        if (isSessionExpired(error)) this.expired = [...this.expired, id];
        this.error = pluginErrorCopy(error);
      });
    }
    this.cancelling = false;
    await this.reloadAccount(id);
  }

  async cancelSignIn(): Promise<void> {
    const signIn = this.signIn;
    if (signIn === null) return;
    this.cancelling = true;
    try {
      await this.ipc.call("integrations.cancelSignIn", { id: signIn.accountId });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = pluginErrorCopy(error);
      });
    }
  }

  async test(): Promise<void> {
    const id = this.accountId;
    if (id === null || this.testing) return;
    this.testing = true;
    this.error = null;
    try {
      const result = await this.ipc.call("integrations.test", { id });
      runInAction(() => {
        this.tests = { ...this.tests, [id]: result };
        this.testing = false;
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.testing = false;
        this.noteFailure(id, error);
      });
    }
    await this.reloadAccount(id);
  }

  async setEnabled(enabled: boolean): Promise<void> {
    const id = this.accountId;
    if (id === null || this.toggling) return;
    this.toggling = true;
    this.error = null;
    try {
      const account = await this.ipc.call(
        enabled ? "integrations.enable" : "integrations.disable",
        { id },
      );
      runInAction(() => {
        this.toggling = false;
        this.merge(account);
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.toggling = false;
        this.noteFailure(id, error);
      });
    }
    await this.reloadAccount(id);
  }

  async remove(): Promise<void> {
    const id = this.accountId;
    const kind = this.kind;
    if (id === null || kind === null || this.removing) return;
    this.removing = true;
    this.error = null;
    try {
      await this.ipc.call("integrations.remove", { id });
      runInAction(() => {
        this.removing = false;
        this.accounts = this.accounts.filter((account) => account.id !== id);
        this.tests = Object.fromEntries(Object.entries(this.tests).filter(([key]) => key !== id));
        this.accountId = null;
        this.form = null;
        this.detail = null;
        this.activity = [];
        this.redirect = pluginPath(kind);
        this.refreshSelection();
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.removing = false;
        this.error = pluginErrorCopy(error);
      });
    }
  }

  resetForm(): void {
    const account = this.selectedAccount;
    if (account !== null) this.form = this.editForm(account);
  }

  dismissError(): void {
    this.error = null;
  }

  private noteFailure(id: IntegrationId, error: unknown): void {
    if (isSessionExpired(error) && !this.expired.includes(id)) {
      this.expired = [...this.expired, id];
    }
    this.error = pluginErrorCopy(error);
  }

  private refreshSelection(): void {
    const kind = this.kind;
    if (!this.loaded || kind === null) return;
    const accounts = this.accountsOf(kind);
    if (this.accountId !== null) {
      const account = accounts.find((entry) => entry.id === this.accountId);
      if (account === undefined) {
        this.accountId = null;
        this.form = null;
        this.redirect = pluginPath(kind);
        this.refreshSelection();
        return;
      }
      this.adding = false;
      this.draftType = null;
      if (this.form?.accountId !== account.id) {
        this.form = this.editForm(account);
        this.detail = null;
        this.activity = [];
        void this.reloadAccount(account.id);
      }
      return;
    }
    const first = accounts[0];
    if (first !== undefined && !this.adding) {
      this.redirect = accountPath(kind, first.id);
      return;
    }
    this.detail = null;
    this.activity = [];
    if (this.form !== null && this.form.isNew && this.form.type.kind === kind) return;
    const types = SIGN_IN_TYPES[kind];
    const only = types.length === 1 ? types[0] : undefined;
    if (only === undefined) {
      this.draftType = null;
      this.form = null;
      return;
    }
    this.chooseType(only.id);
  }

  private editForm(account: IntegrationAccountDto): PluginFormVm {
    const fields = this.kinds.find((entry) => entry.kind === account.kind)?.fields ?? [];
    return new PluginFormVm(fields, signInTypeOf(account), account, account.name);
  }

  private merge(account: IntegrationAccountDto): void {
    const index = this.accounts.findIndex((entry) => entry.id === account.id);
    const plain = plainAccount(account);
    this.accounts =
      index === -1
        ? [...this.accounts, plain]
        : this.accounts.map((entry, at) => (at === index ? plain : entry));
  }

  private async reloadAccount(id: IntegrationId): Promise<void> {
    try {
      const [detail, activity, workers] = await Promise.all([
        this.ipc.call("integrations.get", { id }),
        this.ipc
          .call("integrations.activity", { id, limit: ACTIVITY_LIMIT })
          .catch((): IntegrationActivityDto[] => []),
        this.ipc.call("workers.list", undefined).catch((): WorkerDto[] => this.workers),
      ]);
      runInAction(() => {
        this.merge(detail);
        this.workers = [...workers];
        if (this.accountId !== id) return;
        this.detail = detail;
        this.activity = [...activity];
      });
    } catch {
      runInAction(() => {
        if (this.accountId === id) this.detail = null;
      });
    }
  }

  private onEvent(event: RoutedEvent): void {
    if (event.type !== "step") return;
    const step = event.step;
    if (step.domain !== INTEGRATIONS_DOMAIN || step.phase !== DEVICE_CODE_PHASE) return;
    const signIn = this.signIn;
    if (signIn === null || step.accountId !== signIn.accountId) return;
    this.signIn = {
      accountId: signIn.accountId,
      userCode: typeof step.userCode === "string" ? step.userCode : null,
      verificationUri: typeof step.verificationUri === "string" ? step.verificationUri : null,
      expiresAt: typeof step.expiresAt === "number" ? step.expiresAt : null,
    };
  }
}

function plainAccount(account: IntegrationAccountDto): IntegrationAccountDto {
  return {
    id: account.id,
    kind: account.kind,
    name: account.name,
    enabled: account.enabled,
    config: account.config,
    status: account.status,
    statusDetail: account.statusDetail,
    lastActivityAt: account.lastActivityAt,
    counters: account.counters,
    createdAt: account.createdAt,
    updatedAt: account.updatedAt,
  };
}
