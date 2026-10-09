import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  VectorSearchInput,
  type CatalogueItemDto,
  type Contract,
  type DeviceProfileDto,
  type ModelDto,
  type ProviderSummaryDto,
  type VectorChunkDto,
  type VectorSearchMode,
  type VectorStoreDto,
  type VectorStoreId,
  type VectorSearchResultDto,
  type VectorSourceDto,
  type VectorSourceKind,
  VectorDocumentDto,
  type DocumentId,
  ResourceSampleDto,
  type RunId,
} from "@zvs/shared";
import type { EventRouter, RoutedEvent } from "../../app/EventRouter";
import { estimateEta, smoothRate, type RateState } from "../progressRate";
import VectorStoreFormVm from "./VectorStoreFormVm";
import { neighbourRows, searchRows } from "./searchRows";
import GraphRelationStore from "./GraphRelationStore";
import VectorEvalStore from "./VectorEvalStore";
import { graphRows, type GraphExpansion } from "./graphExpansion";

export default class VectorStoreStore {
  stores: VectorStoreDto[] = [];
  providers: ProviderSummaryDto[] = [];
  embeddingModelsByProvider = new Map<string, readonly ModelDto[]>();
  catalogue: readonly CatalogueItemDto[] = [];
  device: DeviceProfileDto | null = null;
  selectedId: VectorStoreId | null = null;
  detail: VectorStoreDto | null = null;
  form: VectorStoreFormVm | null = null;
  tab = "Test search";
  filter = "";
  query = "";
  k = "5";
  minScore = "0.35";
  searchMode: VectorSearchMode = "hybrid";
  result: VectorSearchResultDto | null = null;
  neighbours = new Map<string, readonly VectorChunkDto[]>();
  neighboursLoading: string | null = null;
  graphExpansions: GraphExpansion[] = [];
  sources: VectorSourceDto[] = [];
  documents: VectorDocumentDto[] = [];
  sourceKind: VectorSourceKind = "folder";
  sourcePath = "";
  sourceInclude = "";
  sourceExclude = "node_modules, .git, dist, build";
  indexRun: { id: RunId; done: number; total: number; note: string } | null = null;
  indexSample: ResourceSampleDto | null = null;
  indexRate: RateState | null = null;
  documentsLoading = false;
  loading = false;
  loaded = false;
  busy = false;
  searching = false;
  error: string | null = null;
  readonly graph: GraphRelationStore;
  readonly evaluation: VectorEvalStore;
  private revision = 0;

  private stopIndexStream: (() => void) | null = null;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly events?: EventRouter,
  ) {
    this.graph = new GraphRelationStore(ipc);
    this.evaluation = new VectorEvalStore(ipc);
    makeAutoObservable<VectorStoreStore, "ipc" | "events">(
      this,
      { ipc: false, events: false, graph: false, evaluation: false },
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
  get hybridAvailable() {
    return (this.detail?.schemaVersion ?? 1) >= 2;
  }
  setSearchMode(mode: string) {
    if (mode === "hybrid" || mode === "vector") this.searchMode = mode;
  }
  neighbourRowsFor(id: string) {
    const row = this.rows.find((candidate) => candidate.id === id);
    const chunks = this.neighbours.get(id);
    return row === undefined || chunks === undefined ? null : neighbourRows(row, chunks);
  }
  async toggleNeighbours(id: string) {
    if (this.neighbours.has(id)) {
      this.neighbours.delete(id);
      return;
    }
    const row = this.rows.find((candidate) => candidate.id === id);
    const storeId = this.detail?.id;
    if (row === undefined || storeId === undefined || this.neighboursLoading !== null) return;
    const revision = this.revision;
    this.neighboursLoading = id;
    try {
      const chunks = await this.ipc.call("vectorStores.chunks", {
        storeId,
        documentId: row.documentId,
        indices: [row.chunkIndex - 1, row.chunkIndex + 1].filter((index) => index >= 0),
      });
      runInAction(() => {
        if (revision === this.revision) this.neighbours.set(id, chunks);
      });
    } catch (error) {
      runInAction(() => {
        this.error = errorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.neighboursLoading = null;
      });
    }
  }
  get graphRows() {
    return graphRows(this.graphExpansions, this.documents);
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
  /** Chunks embedded per second, smoothed so the estimate does not jump between reports. */
  get indexChunksPerSecond(): number {
    return this.indexRate?.perSecond ?? 0;
  }
  /**
   * Milliseconds left, or `undefined` while there is nothing to base it on. The total is an
   * estimate that converges as files are chunked, so this tightens as the run goes on.
   */
  get indexEtaMs(): number | undefined {
    const run = this.indexRun;
    if (!run) return undefined;
    return estimateEta(run.done, run.total, this.indexChunksPerSecond);
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
      const [catalogue, device] = await Promise.all([
        this.optional(() => this.ipc.call("downloads.catalogue", { refresh: false }), []),
        this.optional<DeviceProfileDto | null>(
          () => this.ipc.call("system.device", { refresh: false }),
          null,
        ),
      ]);
      runInAction(() => {
        this.stores = stores;
        this.providers = providers.filter((p) => p.capabilities.includes("embedding"));
        this.embeddingModelsByProvider = new Map(modelEntries);
        this.catalogue = catalogue;
        this.device = device;
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
    this.graphExpansions = [];
    this.sources = [];
    this.documents = [];
    this.searching = false;
    this.graph.reset();
    this.evaluation.reset();
    this.error = null;
    try {
      const detail = await this.ipc.call("vectorStores.get", { id });
      runInAction(() => {
        if (revision === this.revision) {
          this.detail = detail;
          this.searchMode = detail.searchMode;
          this.merge(detail);
        }
      });
      if (revision === this.revision) {
        await this.loadDocuments(id);
        await this.graph.load(id);
        await this.evaluation.load(id);
        await this.restoreIndex(id, revision);
      }
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
    this.form = new VectorStoreFormVm(
      null,
      this.providers,
      this.embeddingModelsByProvider,
      this.formContext,
    );
  }
  edit() {
    if (this.detail && !this.busy)
      this.form = new VectorStoreFormVm(
        this.detail,
        this.providers,
        this.embeddingModelsByProvider,
        this.formContext,
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
  async clear() {
    const id = this.selectedId;
    if (!id || this.indexRun) return false;
    return this.mutate(async () => {
      const detail = await this.ipc.call("vectorStores.clear", { id });
      runInAction(() => {
        this.merge(detail);
        if (this.selectedId === id) this.detail = detail;
        this.documents = [];
        this.result = null;
        this.graphExpansions = [];
      });
      await this.graph.load(id);
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
        this.graphExpansions = [];
        this.form = null;
        this.graph.reset();
        this.evaluation.reset();
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
    const storeId = this.detail.id;
    this.searching = true;
    this.error = null;
    this.result = null;
    this.graphExpansions = [];
    this.neighbours = new Map();
    try {
      const result = await this.ipc.call("vectorStores.searchTimed", {
        ...input.data,
        minScore: 0,
        mode: this.hybridAvailable ? this.searchMode : "vector",
      });
      runInAction(() => {
        if (revision === this.revision) this.result = result;
      });
      const expansions = await this.graph.expand(storeId, result.hits, this.documents);
      runInAction(() => {
        if (revision === this.revision) this.graphExpansions = expansions;
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
      await this.graph.load(storeId);
    });
  }
  async startIndex(full = false) {
    const storeId = this.selectedId;
    if (!storeId || this.busy || this.indexRun) return false;
    return this.mutate(async () => {
      const handle = await this.ipc.call("vectorStores.index", { storeId, full });
      // The document list is kept: a run reports only the files it re-embeds, which move to the
      // top as they arrive. Clearing it would hide every unchanged document until the run ends
      // and read as a wiped index; the refresh at the end drops whatever was removed from disk.
      runInAction(() => {
        this.indexRun = { id: handle.id, done: 0, total: 0, note: "Индексация запущена" };
        this.indexSample = null;
        this.indexRate = null;
      });
      this.stopIndexStream?.();
      this.stopIndexStream =
        this.events?.subscribe(handle.streamId, (event) => {
          this.receiveIndexEvent(storeId, event);
        }) ?? null;
    });
  }
  private async restoreIndex(storeId: VectorStoreId, revision: number) {
    const page = await this.ipc.call("runs.list", {
      live: true,
      kinds: ["indexing"],
      limit: 100,
    });
    const summary = page.items.find(
      (run) => run.subjectId === storeId && ["queued", "running", "blocked"].includes(run.status),
    );
    if (!summary || revision !== this.revision || this.selectedId !== storeId) return;

    this.stopIndexStream?.();
    this.indexRun = {
      id: summary.id,
      done: summary.progress.done,
      total: summary.progress.total,
      note: "Индексация продолжается",
    };
    this.indexSample = null;
    this.indexRate = null;
    const detail = await this.ipc.call("runs.detail", { id: summary.id });
    if (revision !== this.revision || this.selectedId !== storeId) return;
    for (const event of detail.events) this.receiveIndexEvent(storeId, event);
    this.stopIndexStream =
      this.events?.subscribe(summary.streamId, (event) => {
        this.receiveIndexEvent(storeId, event);
      }) ?? null;
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
      if (event.type === "progress") {
        this.indexRun = { ...this.indexRun, done: event.done, total: event.total };
        this.indexRate = smoothRate(this.indexRate ?? undefined, {
          value: event.done,
          at: event.ts,
        });
      } else if (event.type === "log" && typeof event.line.message === "string")
        this.indexRun = { ...this.indexRun, note: event.line.message };
      else if (event.type === "step" && event.step.domain === "resources")
        this.indexSample = readSample(event.step);
      else if (event.type === "step" && event.step.domain === "document")
        this.receiveDocument(event.step);
    });
    if (event.type !== "end") return;
    this.stopIndexStream?.();
    this.stopIndexStream = null;
    runInAction(() => {
      this.indexRun = null;
      this.indexSample = null;
      this.indexRate = null;
      if (event.outcome.status === "failed" && event.outcome.message !== undefined)
        this.error = event.outcome.message;
    });
    void this.refreshAfterIndex(storeId);
  }
  /** Newest first, and a re-indexed file moves rather than appearing twice. */
  receiveDocument(step: Record<string, unknown>) {
    const document = readDocument(step);
    if (document === null) return;
    const rest = this.documents.filter((known) => known.id !== document.id);
    this.documents = [document, ...rest];
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
    await this.graph.load(storeId);
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
  private get formContext() {
    return { catalogue: this.catalogue, device: this.device };
  }
  /**
   * Downloads and the device profile only sharpen auto-fill. A host that cannot answer must
   * still leave the page usable, so neither failure reaches the error banner.
   */
  private async optional<T>(read: () => Promise<T>, fallback: T): Promise<T> {
    try {
      return await read();
    } catch {
      return fallback;
    }
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

/**
 * A resources step is a loose payload on the wire, so it is validated here rather than trusted.
 * A malformed sample is dropped: a missing meter is better than a wrong one.
 */
function readSample(step: Record<string, unknown>): ResourceSampleDto | null {
  const parsed = ResourceSampleDto.safeParse(step);
  return parsed.success ? parsed.data : null;
}

/**
 * A document step is a loose payload on the wire, so it is validated rather than trusted. A
 * malformed one is dropped: the list is rebuilt from the database when the run ends anyway.
 */
function readDocument(step: Record<string, unknown>): VectorDocumentDto | null {
  const parsed = VectorDocumentDto.safeParse(step);
  return parsed.success ? parsed.data : null;
}
