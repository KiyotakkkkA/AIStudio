import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  VectorSearchInput,
  type Contract,
  type ModelDto,
  type ProviderSummaryDto,
  type VectorStoreDto,
  type VectorStoreId,
  type VectorSearchResultDto,
  type VectorSourceDto,
  type VectorSourceKind,
  type VectorDocumentDto,
  type DocumentId,
  type RunId,
} from "@zvs/shared";
import type { EventRouter, RoutedEvent } from "../../app/EventRouter";
import VectorStoreFormVm from "./VectorStoreFormVm";
import { searchRows } from "./searchRows";

export default class VectorStoreStore {
  stores: VectorStoreDto[] = [];
  providers: ProviderSummaryDto[] = [];
  embeddingModelsByProvider = new Map<string, readonly ModelDto[]>();
  selectedId: VectorStoreId | null = null;
  detail: VectorStoreDto | null = null;
  form: VectorStoreFormVm | null = null;
  tab = "Test search";
  filter = "";
  query = "";
  k = "5";
  minScore = "0.35";
  result: VectorSearchResultDto | null = null;
  sources: VectorSourceDto[] = [];
  documents: VectorDocumentDto[] = [];
  sourceKind: VectorSourceKind = "folder";
  sourcePath = "";
  sourceInclude = "";
  sourceExclude = "node_modules, .git, dist, build";
  indexRun: { id: RunId; done: number; total: number; note: string } | null = null;
  documentsLoading = false;
  loading = false;
  loaded = false;
  busy = false;
  searching = false;
  error: string | null = null;
  private revision = 0;

  private stopIndexStream: (() => void) | null = null;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly events?: EventRouter,
  ) {
    makeAutoObservable<VectorStoreStore, "ipc" | "events">(
      this,
      { ipc: false, events: false },
      { autoBind: true },
    );
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
  set(
    field:
      | "filter"
      | "query"
      | "k"
      | "minScore"
      | "tab"
      | "sourcePath"
      | "sourceInclude"
      | "sourceExclude",
    value: string,
  ) {
    this[field] = value;
  }
  setSourceKind(kind: VectorSourceKind) {
    this.sourceKind = kind;
  }
  get indexing() {
    return this.indexRun !== null;
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
      const modelEntries = await Promise.all(
        providers
          .filter((provider) => provider.capabilities.includes("embedding"))
          .map(async (provider) => {
            const detail = await this.ipc.call("providers.get", {
              id: provider.id,
              selectedOnly: true,
            });
            return [provider.id, detail.models] as const;
          }),
      );
      runInAction(() => {
        this.stores = stores;
        this.providers = providers.filter((p) => p.capabilities.includes("embedding"));
        this.embeddingModelsByProvider = new Map(modelEntries);
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
    this.sources = [];
    this.documents = [];
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
      if (revision === this.revision) await this.loadDocuments(id);
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
    this.form = new VectorStoreFormVm(null, this.providers, this.embeddingModelsByProvider);
  }
  edit() {
    if (this.detail && !this.busy)
      this.form = new VectorStoreFormVm(
        this.detail,
        this.providers,
        this.embeddingModelsByProvider,
      );
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
  async loadDocuments(id: VectorStoreId | null = this.selectedId) {
    if (!id) return;
    this.documentsLoading = true;
    try {
      const [sources, documents] = await Promise.all([
        this.ipc.call("vectorStores.sources.list", { id }),
        this.ipc.call("vectorStores.documents.list", { id }),
      ]);
      runInAction(() => {
        if (this.selectedId !== id) return;
        this.sources = sources;
        this.documents = documents;
      });
    } catch (error) {
      runInAction(() => {
        this.error = errorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.documentsLoading = false;
      });
    }
  }
  async pickSource() {
    const id = this.selectedId;
    if (!id || this.busy) return false;
    return this.mutate(async () => {
      const { paths } = await this.ipc.call("vectorStores.sources.pick", {
        kind: this.sourceKind,
      });
      for (const path of paths) await this.submitSource(id, path);
      await this.loadDocuments(id);
    });
  }
  async addSource() {
    const id = this.selectedId;
    const path = this.sourcePath.trim();
    if (!id || !path || this.busy) return false;
    return this.mutate(async () => {
      await this.submitSource(id, path);
      runInAction(() => {
        this.sourcePath = "";
      });
      await this.loadDocuments(id);
    });
  }
  async removeSource(sourceId: string) {
    if (this.busy) return false;
    return this.mutate(async () => {
      await this.ipc.call("vectorStores.sources.remove", { id: sourceId });
      await this.loadDocuments();
    });
  }
  async removeDocument(documentId: DocumentId) {
    const storeId = this.selectedId;
    if (!storeId || this.busy) return false;
    return this.mutate(async () => {
      await this.ipc.call("vectorStores.documents.remove", { storeId, id: documentId });
      const detail = await this.ipc.call("vectorStores.get", { id: storeId });
      runInAction(() => {
        this.merge(detail);
        if (this.selectedId === storeId) this.detail = detail;
      });
      await this.loadDocuments(storeId);
    });
  }
  async startIndex(full = false) {
    const storeId = this.selectedId;
    if (!storeId || this.busy || this.indexRun) return false;
    return this.mutate(async () => {
      const handle = await this.ipc.call("vectorStores.index", { storeId, full });
      runInAction(() => {
        this.indexRun = { id: handle.id, done: 0, total: 0, note: "Индексация запущена" };
      });
      this.stopIndexStream?.();
      this.stopIndexStream =
        this.events?.subscribe(handle.streamId, (event) => {
          this.receiveIndexEvent(storeId, event);
        }) ?? null;
    });
  }
  async cancelIndex() {
    const run = this.indexRun;
    if (!run) return;
    try {
      await this.ipc.call("runs.cancel", { id: run.id });
    } catch (error) {
      runInAction(() => {
        this.error = errorCopy(error);
      });
    }
  }
  private receiveIndexEvent(storeId: VectorStoreId, event: RoutedEvent) {
    runInAction(() => {
      if (!this.indexRun) return;
      if (event.type === "progress")
        this.indexRun = { ...this.indexRun, done: event.done, total: event.total };
      else if (event.type === "log" && typeof event.line.message === "string")
        this.indexRun = { ...this.indexRun, note: event.line.message };
    });
    if (event.type !== "end") return;
    this.stopIndexStream?.();
    this.stopIndexStream = null;
    runInAction(() => {
      this.indexRun = null;
      if (event.outcome.status === "failed" && event.outcome.message !== undefined)
        this.error = event.outcome.message;
    });
    void this.refreshAfterIndex(storeId);
  }
  private async refreshAfterIndex(storeId: VectorStoreId) {
    try {
      const detail = await this.ipc.call("vectorStores.get", { id: storeId });
      runInAction(() => {
        this.merge(detail);
        if (this.selectedId === storeId) this.detail = detail;
      });
    } catch {
      /* the detail refresh is best effort; the store list stays as it was */
    }
    await this.loadDocuments(storeId);
  }
  private async submitSource(storeId: VectorStoreId, path: string) {
    await this.ipc.call("vectorStores.sources.add", {
      storeId,
      kind: this.sourceKind,
      path,
      include: splitPatterns(this.sourceInclude),
      exclude: splitPatterns(this.sourceExclude),
      recursive: true,
    });
  }
  dispose() {
    this.stopIndexStream?.();
    this.stopIndexStream = null;
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

function splitPatterns(value: string): string[] {
  return value
    .split(/[\n,]/)
    .map((pattern) => pattern.trim())
    .filter((pattern) => pattern.length > 0);
}

function errorCopy(error: unknown) {
  return error instanceof Error
    ? error.message
    : "Не удалось выполнить операцию. Попробуйте ещё раз.";
}
