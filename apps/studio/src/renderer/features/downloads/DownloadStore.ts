import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  TERMINAL_DOWNLOAD_STATUSES,
  type CatalogueItemDto,
  type CatalogueItemState,
  type Contract,
  type DiskUsageDto,
  type DownloadDto,
  type DownloadId,
  type DownloadItemKind,
  type Json,
  type StreamId,
} from "@zvs/shared";
import type { EventRouter, RoutedEvent } from "../../app/EventRouter";
import { estimateEta, smoothRate, type RateState } from "./rateSmoothing";

const CONCURRENCY_KEY = "downloads.concurrency";
const OPTIONS_KEY = "downloads.options";
const DEFAULT_CONCURRENCY = 2;
const PRIORITY_MIN = 0;
const PRIORITY_MAX = 9;

export interface PostDownloadOptions {
  readonly verifyChecksum: boolean;
}

const DEFAULT_OPTIONS: PostDownloadOptions = { verifyChecksum: true };

export default class DownloadStore {
  downloads: DownloadDto[] = [];
  catalogue: CatalogueItemDto[] = [];
  disk: DiskUsageDto | null = null;
  concurrency = DEFAULT_CONCURRENCY;
  kind: DownloadItemKind | "all" = "all";
  state: CatalogueItemState | null = null;
  query = "";
  selectedRef: string | null = null;
  rates: Record<string, RateState> = {};
  options: Record<string, PostDownloadOptions> = {};
  loading = false;
  loaded = false;
  refreshing = false;
  busyId: string | null = null;
  error: string | null = null;
  catalogueError: string | null = null;
  private readonly streams = new Map<StreamId, DownloadId>();
  private unobserve: (() => void) | undefined;
  private refreshTimer: ReturnType<typeof setTimeout> | undefined;
  private mounted = false;
  private revision = 0;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly events: EventRouter,
  ) {
    makeAutoObservable<
      DownloadStore,
      "ipc" | "events" | "streams" | "unobserve" | "refreshTimer" | "mounted" | "revision"
    >(
      this,
      {
        ipc: false,
        events: false,
        streams: false,
        unobserve: false,
        refreshTimer: false,
        mounted: false,
        revision: false,
      },
      { autoBind: true },
    );
  }

  get active(): DownloadDto[] {
    return this.downloads.filter((row) => row.status === "running" || row.status === "paused");
  }

  get queued(): DownloadDto[] {
    return [...this.downloads.filter((row) => row.status === "queued")].sort(
      (left, right) => right.priority - left.priority || left.sizeBytes - right.sizeBytes,
    );
  }

  get failed(): DownloadDto[] {
    return this.downloads.filter((row) => row.status === "failed");
  }

  get runningCount(): number {
    return this.downloads.filter((row) => row.status === "running").length;
  }

  get totalRate(): number {
    return this.active.reduce((sum, row) => sum + this.rateOf(row), 0);
  }

  get visible(): CatalogueItemDto[] {
    const query = this.query.trim().toLowerCase();
    return this.catalogue.filter(
      (item) =>
        (this.kind === "all" || item.kind === this.kind) &&
        (this.state === null || item.state === this.state) &&
        (query === "" ||
          item.displayName.toLowerCase().includes(query) ||
          item.name.toLowerCase().includes(query)),
    );
  }

  get kindCounts(): Record<DownloadItemKind, number> {
    const counts: Record<DownloadItemKind, number> = { model: 0, embedding: 0, mcp: 0, skill: 0 };
    for (const item of this.catalogue) counts[item.kind] += 1;
    return counts;
  }

  get stateCounts(): Record<CatalogueItemState, number> {
    const counts: Record<CatalogueItemState, number> = {
      installed: 0,
      update: 0,
      available: 0,
      downloading: 0,
      queued: 0,
    };
    for (const item of this.catalogue) counts[item.state] += 1;
    return counts;
  }

  get selectedItem(): CatalogueItemDto | null {
    return this.catalogue.find((item) => item.ref === this.selectedRef) ?? null;
  }

  get selectedDownload(): DownloadDto | null {
    if (this.selectedRef === null) return null;
    const rows = this.downloads.filter((row) => row.itemRef === this.selectedRef);
    return rows.find((row) => !TERMINAL_DOWNLOAD_STATUSES.includes(row.status)) ?? rows[0] ?? null;
  }

  rateOf(row: DownloadDto): number {
    if (row.status !== "running") return 0;
    return this.rates[row.id]?.bytesPerSecond ?? row.rateBytesPerSecond;
  }

  etaOf(row: DownloadDto): number | undefined {
    return estimateEta(row.bytesDone, row.sizeBytes, this.rateOf(row));
  }

  installedId(ref: string): DownloadId | undefined {
    return this.downloads.find((row) => row.itemRef === ref && row.status === "succeeded")?.id;
  }

  optionsFor(ref: string): PostDownloadOptions {
    return this.options[ref] ?? DEFAULT_OPTIONS;
  }

  setKind(value: DownloadItemKind | "all") {
    this.kind = value;
  }

  setState(value: CatalogueItemState | null) {
    this.state = this.state === value ? null : value;
  }

  setQuery(value: string) {
    this.query = value;
  }

  select(ref: string | null) {
    this.selectedRef = ref;
  }

  async mount() {
    if (this.mounted) return;
    this.mounted = true;
    this.unobserve = this.events.observe(this.receive);
    await this.load();
  }

  async load() {
    const revision = ++this.revision;
    this.loading = true;
    this.error = null;
    const [rows, catalogue, disk, streams, concurrency, options] = await Promise.all([
      this.ipc.call("downloads.list", {}).catch(this.keep("error", [] as DownloadDto[])),
      this.ipc
        .call("downloads.catalogue", { refresh: false })
        .catch(this.keep("catalogueError", [] as CatalogueItemDto[])),
      this.ipc.call("downloads.disk", { refresh: false }).catch(() => null),
      this.readStreams(),
      this.readConcurrency(),
      this.readOptions(),
    ]);
    runInAction(() => {
      if (revision !== this.revision) return;
      this.downloads = rows;
      if (this.catalogueError === null) this.catalogue = catalogue;
      if (disk !== null) this.disk = disk;
      this.concurrency = concurrency;
      this.options = options;
      this.streams.clear();
      for (const [streamId, downloadId] of streams) this.streams.set(streamId, downloadId);
      for (const id of Object.keys(this.rates))
        if (!rows.some((row) => row.id === id && row.status === "running")) delete this.rates[id];
      this.loading = false;
      this.loaded = true;
    });
  }

  async refresh() {
    if (this.refreshing) return;
    this.refreshing = true;
    this.catalogueError = null;
    try {
      const [catalogue, disk] = await Promise.all([
        this.ipc.call("downloads.catalogue", { refresh: true }),
        this.ipc.call("downloads.disk", { refresh: true }).catch(() => null),
      ]);
      runInAction(() => {
        this.catalogue = catalogue;
        if (disk !== null) this.disk = disk;
      });
    } catch (error) {
      runInAction(() => {
        this.catalogueError = copy(error, "Не удалось обновить каталог.");
      });
    } finally {
      runInAction(() => {
        this.refreshing = false;
      });
    }
  }

  start(ref: string) {
    this.selectedRef = ref;
    return this.act(ref, () => this.ipc.call("downloads.start", { ref }));
  }

  pause(id: DownloadId) {
    return this.act(id, () => this.ipc.call("downloads.pause", { id }));
  }

  resume(id: DownloadId) {
    return this.act(id, () => this.ipc.call("downloads.resume", { id }));
  }

  cancel(id: DownloadId) {
    return this.act(id, () => this.ipc.call("downloads.cancel", { id }));
  }

  remove(id: DownloadId) {
    return this.act(id, () => this.ipc.call("downloads.remove", { id }));
  }

  nudge(id: DownloadId, direction: 1 | -1) {
    const row = this.downloads.find((item) => item.id === id);
    if (!row) return Promise.resolve();
    const priority = Math.min(PRIORITY_MAX, Math.max(PRIORITY_MIN, row.priority + direction));
    if (priority === row.priority) return Promise.resolve();
    return this.act(id, () => this.ipc.call("downloads.prioritise", { id, priority }));
  }

  async pauseAll() {
    const running = this.downloads.filter((row) => row.status === "running");
    const first = running[0];
    if (first === undefined) return;
    this.busyId = first.id;
    try {
      for (const row of running) await this.ipc.call("downloads.pause", { id: row.id });
    } catch (error) {
      runInAction(() => {
        this.error = copy(error, "Не удалось приостановить загрузки.");
      });
    } finally {
      runInAction(() => {
        this.busyId = null;
      });
      await this.load();
    }
  }

  async setOption(ref: string, patch: Partial<PostDownloadOptions>) {
    const next = { ...this.optionsFor(ref), ...patch };
    this.options = { ...this.options, [ref]: next };
    const value: Json = Object.fromEntries(
      Object.entries(this.options).map(([key, entry]) => [
        key,
        { verifyChecksum: entry.verifyChecksum },
      ]),
    );
    try {
      await this.ipc.call("settings.set", { key: OPTIONS_KEY, value });
    } catch (error) {
      runInAction(() => {
        this.error = copy(error, "Не удалось сохранить настройку.");
      });
    }
  }

  dispose() {
    this.mounted = false;
    ++this.revision;
    this.unobserve?.();
    this.unobserve = undefined;
    this.streams.clear();
    if (this.refreshTimer !== undefined) clearTimeout(this.refreshTimer);
    this.refreshTimer = undefined;
    this.loading = false;
  }

  receive(event: RoutedEvent) {
    const id = this.streams.get(event.streamId);
    if (id === undefined) {
      if (event.type === "progress" || event.type === "end") this.scheduleRefresh();
      return;
    }
    if (event.gap !== undefined) this.scheduleRefresh();
    if (event.type === "end") {
      this.streams.delete(event.streamId);
      delete this.rates[id];
      this.scheduleRefresh();
      return;
    }
    if (event.type !== "progress") return;
    const row = this.downloads.find((item) => item.id === id);
    if (!row) return;
    row.bytesDone = event.done;
    if (event.total > 0) row.sizeBytes = event.total;
    this.rates[id] = smoothRate(this.rates[id], { bytes: event.done, at: event.ts });
  }

  private async act(key: string, action: () => Promise<unknown>) {
    if (this.busyId !== null) return;
    this.busyId = key;
    this.error = null;
    try {
      await action();
    } catch (error) {
      runInAction(() => {
        this.error = copy(error, "Не удалось выполнить действие.");
      });
    } finally {
      runInAction(() => {
        this.busyId = null;
      });
      await this.load();
    }
  }

  private async readStreams(): Promise<readonly (readonly [StreamId, DownloadId])[]> {
    try {
      const page = await this.ipc.call("runs.list", {
        kinds: ["download"],
        live: true,
        limit: 50,
      });
      return page.items.flatMap((run) =>
        run.subjectId === undefined ? [] : [[run.streamId, run.subjectId as DownloadId] as const],
      );
    } catch {
      return [];
    }
  }

  private async readConcurrency(): Promise<number> {
    try {
      const setting = await this.ipc.call("settings.get", { key: CONCURRENCY_KEY });
      return typeof setting.value === "number" && setting.value >= 1
        ? setting.value
        : DEFAULT_CONCURRENCY;
    } catch {
      return DEFAULT_CONCURRENCY;
    }
  }

  private async readOptions(): Promise<Record<string, PostDownloadOptions>> {
    try {
      const setting = await this.ipc.call("settings.get", { key: OPTIONS_KEY });
      return readStoredOptions(setting.value);
    } catch {
      return {};
    }
  }

  private keep<T>(field: "error" | "catalogueError", fallback: T) {
    return (error: unknown): T => {
      runInAction(() => {
        this[field] = copy(
          error,
          field === "catalogueError" ? "Каталог недоступен." : "Не удалось получить загрузки.",
        );
      });
      return fallback;
    };
  }

  private scheduleRefresh() {
    if (!this.mounted || this.refreshTimer !== undefined) return;
    this.refreshTimer = setTimeout(() => {
      this.refreshTimer = undefined;
      void this.load();
    }, 250);
  }
}

function readStoredOptions(value: unknown): Record<string, PostDownloadOptions> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return {};
  const parsed: Record<string, PostDownloadOptions> = {};
  for (const [ref, raw] of Object.entries(value)) {
    if (raw === null || typeof raw !== "object" || Array.isArray(raw)) continue;
    const entry = raw as Record<string, unknown>;
    parsed[ref] = { verifyChecksum: entry.verifyChecksum !== false };
  }
  return parsed;
}

function copy(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}
