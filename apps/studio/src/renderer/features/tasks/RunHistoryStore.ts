import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type {
  Contract,
  RunCountsDto,
  RunDetailDto,
  RunDto,
  RunId,
  RunSummaryDto,
} from "@zvs/shared";

export type RunRangeKey = "all" | "today" | "week" | "month";

const DAY_MS = 24 * 60 * 60 * 1000;
const RANGES: Record<RunRangeKey, number | undefined> = {
  all: undefined,
  today: DAY_MS,
  week: 7 * DAY_MS,
  month: 30 * DAY_MS,
};

const EMPTY_COUNTS: RunCountsDto = {
  total: 0,
  byStatus: {
    queued: 0,
    running: 0,
    blocked: 0,
    succeeded: 0,
    failed: 0,
    cancelled: 0,
    interrupted: 0,
  },
  byKind: { chat: 0, scenario: 0, agentic: 0, job: 0, indexing: 0, browser: 0 },
};

const PAGE_SIZE = 40;

export default class RunHistoryStore {
  runs: RunSummaryDto[] = [];
  counts: RunCountsDto = EMPTY_COUNTS;
  query = "";
  statuses: RunDto["status"][] = [];
  kinds: RunDto["kind"][] = [];
  range: RunRangeKey = "all";
  cursor: string | undefined = undefined;
  selectedId: RunId | null = null;
  detail: RunDetailDto | null = null;
  expandedStepId: string | null = null;
  loading = false;
  loaded = false;
  loadingMore = false;
  loadingDetail = false;
  error: string | null = null;
  private searchTimer: ReturnType<typeof setTimeout> | undefined;
  private revision = 0;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<RunHistoryStore, "ipc" | "searchTimer" | "revision">(
      this,
      { ipc: false, searchTimer: false, revision: false },
      { autoBind: true },
    );
  }

  get hasMore(): boolean {
    return this.cursor !== undefined;
  }
  get filtered(): boolean {
    return (
      this.query.trim().length > 0 ||
      this.statuses.length > 0 ||
      this.kinds.length > 0 ||
      this.range !== "all"
    );
  }

  async mount() {
    await this.load();
  }

  dispose() {
    ++this.revision;
    if (this.searchTimer !== undefined) clearTimeout(this.searchTimer);
    this.searchTimer = undefined;
    this.loading = false;
  }

  setQuery(value: string) {
    this.query = value;
    if (this.searchTimer !== undefined) clearTimeout(this.searchTimer);
    this.searchTimer = setTimeout(() => {
      this.searchTimer = undefined;
      void this.load();
    }, 250);
  }

  toggleStatus(status: RunDto["status"]) {
    this.statuses = this.statuses.includes(status)
      ? this.statuses.filter((item) => item !== status)
      : [...this.statuses, status];
    void this.load();
  }

  toggleKind(kind: RunDto["kind"]) {
    this.kinds = this.kinds.includes(kind)
      ? this.kinds.filter((item) => item !== kind)
      : [...this.kinds, kind];
    void this.load();
  }

  setRange(range: RunRangeKey) {
    this.range = range;
    void this.load();
  }

  reset() {
    this.query = "";
    this.statuses = [];
    this.kinds = [];
    this.range = "all";
    void this.load();
  }

  async load() {
    const revision = ++this.revision;
    this.loading = true;
    this.error = null;
    try {
      const page = await this.ipc.call("runs.list", this.filter());
      runInAction(() => {
        if (revision !== this.revision) return;
        this.runs = page.items;
        this.counts = page.counts;
        this.cursor = page.nextCursor;
        this.loading = false;
        this.loaded = true;
      });
    } catch (error) {
      runInAction(() => {
        if (revision !== this.revision) return;
        this.error = errorCopy(error);
        this.loading = false;
      });
    }
  }

  async loadMore() {
    if (this.cursor === undefined || this.loadingMore) return;
    const revision = this.revision;
    this.loadingMore = true;
    try {
      const page = await this.ipc.call("runs.list", { ...this.filter(), cursor: this.cursor });
      runInAction(() => {
        if (revision !== this.revision) return;
        this.runs = [...this.runs, ...page.items];
        this.cursor = page.nextCursor;
      });
    } catch (error) {
      runInAction(() => {
        if (revision === this.revision) this.error = errorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.loadingMore = false;
      });
    }
  }

  async select(id: RunId) {
    if (this.selectedId === id) return;
    this.selectedId = id;
    this.detail = null;
    this.expandedStepId = null;
    this.loadingDetail = true;
    try {
      const detail = await this.ipc.call("runs.detail", { id });
      runInAction(() => {
        if (this.selectedId !== id) return;
        this.detail = detail;
        this.loadingDetail = false;
      });
    } catch (error) {
      runInAction(() => {
        if (this.selectedId !== id) return;
        this.error = errorCopy(error);
        this.loadingDetail = false;
      });
    }
  }

  toggleStep(stepId: string) {
    this.expandedStepId = this.expandedStepId === stepId ? null : stepId;
  }

  private filter() {
    const window = RANGES[this.range];
    const query = this.query.trim();
    return {
      live: false,
      limit: PAGE_SIZE,
      ...(this.statuses.length ? { statuses: [...this.statuses] } : {}),
      ...(this.kinds.length ? { kinds: [...this.kinds] } : {}),
      ...(query ? { query } : {}),
      ...(window === undefined ? {} : { from: Math.max(0, Date.now() - window) }),
    };
  }
}

function errorCopy(error: unknown): string {
  return error instanceof Error ? error.message : "Не удалось загрузить историю запусков.";
}
