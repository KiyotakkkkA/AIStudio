import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type {
  Contract,
  CreateSecretInput,
  SecretDto,
  SecretId,
  SecretSummaryDto,
  SecretTypeSchema,
  UpdateSecretInput,
} from "@zvs/shared";
import { consumersFrom, errorCopy, fieldErrorsFrom, type SecretConsumerRef } from "./errorCopy";
import SecretFormVm from "./SecretFormVm";

export const SCOPE_FILTERS = ["all", "personal", "shared", "public"] as const;
export type ScopeFilter = (typeof SCOPE_FILTERS)[number];

export interface SecretConflict {
  readonly id: SecretId;
  readonly name: string;
  readonly consumers: readonly SecretConsumerRef[];
}

export type PendingIntent =
  { readonly kind: "create" } | { readonly kind: "select"; readonly id: SecretId };

export class SecretStore {
  summaries: SecretSummaryDto[] = [];
  types: SecretTypeSchema[] = [];
  selectedId: SecretId | null = null;
  selected: SecretDto | null = null;
  form: SecretFormVm | null = null;
  pending: PendingIntent | null = null;
  filter: ScopeFilter = "all";
  query = "";
  loading = false;
  loaded = false;
  saving = false;
  removing = false;
  error: string | null = null;
  conflict: SecretConflict | null = null;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<SecretStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  get matchingQuery(): SecretSummaryDto[] {
    const query = this.query.trim().toLowerCase();
    if (query.length === 0) return this.summaries;
    return this.summaries.filter(
      (secret) =>
        secret.name.toLowerCase().includes(query) ||
        secret.type.toLowerCase().includes(query) ||
        secret.tags.some((tag) => tag.toLowerCase().includes(query)),
    );
  }

  get visible(): SecretSummaryDto[] {
    if (this.filter === "all") return this.matchingQuery;
    return this.matchingQuery.filter((secret) => secret.scope === this.filter);
  }

  get counts(): Record<ScopeFilter, number> {
    const counts: Record<ScopeFilter, number> = { all: 0, personal: 0, shared: 0, public: 0 };
    for (const secret of this.matchingQuery) {
      counts.all += 1;
      counts[secret.scope] += 1;
    }
    return counts;
  }

  get total(): number {
    return this.summaries.length;
  }

  get isEmpty(): boolean {
    return this.loaded && this.summaries.length === 0;
  }

  get ready(): boolean {
    return this.types.length > 0;
  }

  schemaOf(type: string): SecretTypeSchema | undefined {
    return this.types.find((schema) => schema.key === type);
  }

  async load(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    this.error = null;
    try {
      const types =
        this.types.length > 0 ? this.types : await this.ipc.call("secrets.types", undefined);
      const summaries = await this.ipc.call("secrets.list", {});
      runInAction(() => {
        this.types = [...types];
        this.summaries = [...summaries];
        this.loaded = true;
        this.loading = false;
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = errorCopy(error);
        this.loaded = true;
        this.loading = false;
      });
    }
  }

  async select(id: SecretId): Promise<void> {
    this.selectedId = id;
    this.selected = null;
    this.form = null;
    try {
      const secret = await this.ipc.call("secrets.get", { id });
      runInAction(() => {
        if (this.selectedId !== id) return;
        this.selected = secret;
        this.form = new SecretFormVm(this.types, secret);
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = errorCopy(error);
        if (this.selectedId === id) this.selectedId = null;
      });
    }
  }

  startCreate(): void {
    if (!this.ready) return;
    this.selectedId = null;
    this.selected = null;
    this.form = new SecretFormVm(this.types);
  }

  requestCreate(): void {
    if (this.form?.dirty === true) {
      this.pending = { kind: "create" };
      return;
    }
    this.startCreate();
  }

  requestSelect(id: SecretId): void {
    if (id === this.selectedId) return;
    if (this.form?.dirty === true) {
      this.pending = { kind: "select", id };
      return;
    }
    void this.select(id);
  }

  confirmPending(): void {
    const intent = this.pending;
    this.pending = null;
    if (intent === null) return;
    if (intent.kind === "create") this.startCreate();
    else void this.select(intent.id);
  }

  cancelPending(): void {
    this.pending = null;
  }

  resetForm(): void {
    if (this.selected === null) {
      this.form = null;
      return;
    }
    this.form = new SecretFormVm(this.types, this.selected);
  }

  setFilter(filter: ScopeFilter): void {
    this.filter = filter;
  }

  setQuery(query: string): void {
    this.query = query;
  }

  dismissError(): void {
    this.error = null;
  }

  dismissConflict(): void {
    this.conflict = null;
  }

  async submit(): Promise<boolean> {
    const form = this.form;
    if (form === null || this.saving) return false;
    form.setBanner(null);
    if (!form.validate()) return false;
    return form.isNew
      ? await this.create(form.toCreateInput())
      : await this.update(form.toUpdateInput());
  }

  async create(input: CreateSecretInput): Promise<boolean> {
    return await this.persist(async () => await this.ipc.call("secrets.create", input));
  }

  async update(input: UpdateSecretInput): Promise<boolean> {
    return await this.persist(async () => await this.ipc.call("secrets.update", input));
  }

  async removeSelected(): Promise<boolean> {
    if (this.selectedId === null) return false;
    return await this.remove(this.selectedId);
  }

  async remove(id: SecretId): Promise<boolean> {
    if (this.removing) return false;
    this.removing = true;
    this.error = null;
    try {
      await this.ipc.call("secrets.remove", { id });
      runInAction(() => {
        this.summaries = this.summaries.filter((secret) => secret.id !== id);
        this.removing = false;
        this.conflict = null;
        if (this.selectedId === id) {
          this.selectedId = null;
          this.selected = null;
          this.form = null;
        }
      });
      return true;
    } catch (error: unknown) {
      const consumers = consumersFrom(error);
      runInAction(() => {
        this.removing = false;
        if (consumers.length > 0) {
          this.conflict = {
            id,
            name: this.summaries.find((secret) => secret.id === id)?.name ?? "",
            consumers,
          };
          return;
        }
        this.error = errorCopy(error);
      });
      return false;
    }
  }

  private async persist(call: () => Promise<SecretDto>): Promise<boolean> {
    this.saving = true;
    this.error = null;
    try {
      const secret = await call();
      runInAction(() => {
        this.saving = false;
        this.applySaved(secret);
      });
      return true;
    } catch (error: unknown) {
      const fieldErrors = fieldErrorsFrom(error);
      const hasFieldErrors = Object.keys(fieldErrors).length > 0;
      runInAction(() => {
        this.saving = false;
        if (this.form === null) {
          this.error = errorCopy(error);
          return;
        }
        this.form.setErrors(fieldErrors);
        this.form.setBanner(hasFieldErrors ? null : errorCopy(error));
      });
      return false;
    }
  }

  private applySaved(secret: SecretDto): void {
    const summary: SecretSummaryDto = {
      id: secret.id,
      type: secret.type,
      name: secret.name,
      scope: secret.scope,
      hint: secret.hint,
      tags: [...secret.tags],
      usageCount: secret.usageCount,
      rotationStatus: secret.rotationStatus,
      rotatesAt: secret.rotatesAt,
      updatedAt: secret.updatedAt,
    };
    const index = this.summaries.findIndex((item) => item.id === secret.id);
    if (index === -1) this.summaries = [summary, ...this.summaries];
    else this.summaries = this.summaries.map((item, at) => (at === index ? summary : item));
    this.selectedId = secret.id;
    this.selected = secret;
    this.form = new SecretFormVm(this.types, secret);
  }
}

export default SecretStore;
