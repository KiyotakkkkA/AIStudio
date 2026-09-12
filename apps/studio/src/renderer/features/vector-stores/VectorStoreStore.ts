import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  VectorSearchInput,
  type Contract,
  type ProviderSummaryDto,
  type VectorStoreDto,
  type VectorStoreId,
  type VectorSearchResultDto,
} from "@zvs/shared";
import VectorStoreFormVm from "./VectorStoreFormVm";
import { searchRows } from "./searchRows";

export default class VectorStoreStore {
  stores: VectorStoreDto[] = [];
  providers: ProviderSummaryDto[] = [];
  selectedId: VectorStoreId | null = null;
  detail: VectorStoreDto | null = null;
  form: VectorStoreFormVm | null = null;
  tab = "Test search";
  filter = "";
  query = "";
  k = "5";
  minScore = "0.35";
  result: VectorSearchResultDto | null = null;
  loading = false;
  loaded = false;
  busy = false;
  searching = false;
  error: string | null = null;
  private revision = 0;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<VectorStoreStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }
  get visibleStores() {
    const q = this.filter.trim().toLowerCase();
    return this.stores.filter((s) => `${s.name} ${s.description}`.toLowerCase().includes(q));
  }
  get rows() {
    return searchRows(this.result?.hits ?? [], Number(this.minScore));
  }
  get hitCount() {
    return this.rows.filter((r) => !r.belowFloor).length;
  }
  set(field: "filter" | "query" | "k" | "minScore" | "tab", value: string) {
    this[field] = value;
  }
  async load() {
    if (this.loading) return;
    this.loading = true;
    this.error = null;
    try {
      const [stores, providers] = await Promise.all([
        this.ipc.call("vectorStores.list", undefined),
        this.ipc.call("providers.list", { capability: "embedding" }),
      ]);
      runInAction(() => {
        this.stores = stores;
        this.providers = providers.filter((p) => p.capabilities.includes("embedding"));
      });
      if (!this.form) {
        const id = this.selectedId ?? stores[0]?.id;
        if (id) await this.select(id);
      }
    } catch (error) {
      runInAction(() => {
        this.error = errorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.loading = false;
        this.loaded = true;
      });
    }
  }
  async select(id: VectorStoreId) {
    if (this.busy || this.form) return;
    const revision = ++this.revision;
    this.selectedId = id;
    this.detail = null;
    this.result = null;
    this.searching = false;
    this.error = null;
    try {
      const detail = await this.ipc.call("vectorStores.get", { id });
      runInAction(() => {
        if (revision === this.revision) {
          this.detail = detail;
          this.merge(detail);
        }
      });
    } catch (error) {
      runInAction(() => {
        if (revision === this.revision) this.error = errorCopy(error);
      });
    }
  }
  create() {
    if (this.busy || this.form) return;
    ++this.revision;
    this.searching = false;
    this.form = new VectorStoreFormVm(null, this.providers);
  }
  edit() {
    if (this.detail && !this.busy) this.form = new VectorStoreFormVm(this.detail, this.providers);
  }
  cancel() {
    if (!this.busy) this.form = null;
  }
  async save() {
    const form = this.form;
    if (!form || this.busy || !form.validate()) return false;
    return this.mutate(async () => {
      const saved = form.isNew
        ? await this.ipc.call("vectorStores.create", form.toCreateInput())
        : await this.ipc.call("vectorStores.update", form.toUpdateInput());
      runInAction(() => {
        this.merge(saved);
        this.selectedId = saved.id;
        this.detail = saved;
        this.form = null;
        this.result = null;
      });
      const detail = await this.ipc.call("vectorStores.get", { id: saved.id });
      runInAction(() => {
        this.merge(detail);
        this.selectedId = detail.id;
        this.detail = detail;
        this.form = null;
        this.result = null;
      });
    });
  }
  async reconcile(all = false) {
    const ids = all ? this.stores.map((s) => s.id) : this.selectedId ? [this.selectedId] : [];
    return this.mutate(async () => {
      for (const id of ids) {
        const detail = await this.ipc.call("vectorStores.reconcile", { id });
        runInAction(() => {
          this.merge(detail);
          if (this.selectedId === id) this.detail = detail;
        });
      }
    });
  }
  async remove() {
    const id = this.selectedId;
    if (!id) return false;
    return this.mutate(async () => {
      await this.ipc.call("vectorStores.remove", { id });
      runInAction(() => {
        ++this.revision;
        this.stores = this.stores.filter((s) => s.id !== id);
        this.selectedId = null;
        this.detail = null;
        this.result = null;
        this.form = null;
      });
    });
  }
  async search() {
    if (this.searching || this.busy || !this.detail || this.form) return;
    const input = VectorSearchInput.safeParse({
      storeId: this.detail.id,
      query: this.query,
      k: Number(this.k),
      minScore: Number(this.minScore),
    });
    if (!input.success || !this.minScore.trim()) {
      this.error = "Укажите запрос, Top K от 1 до 1000 и порог от 0 до 1.";
      return;
    }
    const revision = ++this.revision;
    this.searching = true;
    this.error = null;
    this.result = null;
    try {
      const result = await this.ipc.call("vectorStores.searchTimed", {
        ...input.data,
        minScore: 0,
      });
      runInAction(() => {
        if (revision === this.revision) this.result = result;
      });
    } catch (error) {
      runInAction(() => {
        if (revision === this.revision) this.error = errorCopy(error);
      });
    } finally {
      runInAction(() => {
        if (revision === this.revision) this.searching = false;
      });
    }
  }
  private merge(detail: VectorStoreDto) {
    this.stores = this.stores.some((s) => s.id === detail.id)
      ? this.stores.map((s) => (s.id === detail.id ? detail : s))
      : [...this.stores, detail];
  }
  private async mutate(work: () => Promise<void>) {
    if (this.busy || this.searching) return false;
    this.busy = true;
    this.error = null;
    try {
      await work();
      return true;
    } catch (error) {
      runInAction(() => {
        this.error = errorCopy(error);
      });
      return false;
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }
}

function errorCopy(error: unknown) {
  return error instanceof Error
    ? error.message
    : "Не удалось выполнить операцию. Попробуйте ещё раз.";
}
