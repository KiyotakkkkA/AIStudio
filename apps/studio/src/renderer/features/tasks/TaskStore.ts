import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  RunApprovalDto,
  type Contract,
  type Json,
  type RunCountsDto,
  type RunDto,
  type RunGraph,
  type RunId,
  type RunSummaryDto,
  type StepDto,
  type StreamId,
} from "@zvs/shared";
import type { EventRouter, RoutedEvent } from "../../app/EventRouter";
import {
  groupRuns,
  matchesStatusFilter,
  statusesOf,
  type TaskStatusFilter,
} from "./runGroups";
import { parseStepEvent } from "./stepEvent";

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
  byKind: { chat: 0, scenario: 0, agentic: 0, job: 0, indexing: 0, download: 0, browser: 0 },
};

export default class TaskStore {
  runs: RunSummaryDto[] = [];
  counts: RunCountsDto = EMPTY_COUNTS;
  status: TaskStatusFilter = "active";
  kind: RunDto["kind"] | "all" = "all";
  selectedId: RunId | null = null;
  steps: StepDto[] = [];
  graph: RunGraph | null = null;
  payload: Json = null;
  now = Date.now();
  loading = false;
  loaded = false;
  clearing = false;
  busyId: string | null = null;
  error: string | null = null;
  private readonly seen = new Map<StreamId, number>();
  private unobserve: (() => void) | undefined;
  private ticker: ReturnType<typeof setInterval> | undefined;
  private refreshTimer: ReturnType<typeof setTimeout> | undefined;
  private mounted = false;
  private revision = 0;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly events: EventRouter,
  ) {
    makeAutoObservable<
      TaskStore,
      | "ipc"
      | "events"
      | "seen"
      | "unobserve"
      | "ticker"
      | "refreshTimer"
      | "mounted"
      | "revision"
    >(
      this,
      {
        ipc: false,
        events: false,
        seen: false,
        unobserve: false,
        ticker: false,
        refreshTimer: false,
        mounted: false,
        revision: false,
      },
      { autoBind: true },
    );
  }

  get visible(): RunSummaryDto[] {
    return this.runs.filter(
      (run) =>
        matchesStatusFilter(run, this.status) && (this.kind === "all" || run.kind === this.kind),
    );
  }
  get groups() {
    return groupRuns(this.visible);
  }
  get selected(): RunSummaryDto | null {
    return this.runs.find((run) => run.id === this.selectedId) ?? null;
  }
  get statusCounts(): Record<TaskStatusFilter, number> {
    const counts: Record<TaskStatusFilter, number> = {
      active: 0,
      running: 0,
      queued: 0,
      blocked: 0,
      failed: 0,
    };
    for (const key of Object.keys(counts) as TaskStatusFilter[])
      counts[key] = this.runs.filter((run) => statusesOf(key).includes(run.status)).length;
    return counts;
  }
  get kindCounts(): Record<RunDto["kind"], number> {
    const counts: Record<RunDto["kind"], number> = {
      chat: 0,
      scenario: 0,
      agentic: 0,
      job: 0,
      indexing: 0,
      download: 0,
      browser: 0,
    };
    for (const run of this.runs) counts[run.kind] += 1;
    return counts;
  }

  setStatus(value: TaskStatusFilter) {
    this.status = value;
  }
  setKind(value: RunDto["kind"] | "all") {
    this.kind = value;
  }

  async mount() {
    if (this.mounted) return;
    this.mounted = true;
    this.unobserve = this.events.observe(this.receive);
    this.ticker = setInterval(() => {
      runInAction(() => {
        this.now = Date.now();
      });
    }, 1000);
    await this.load();
  }

  async load() {
    const revision = ++this.revision;
    this.loading = true;
    this.error = null;
    try {
      const page = await this.ipc.call("runs.list", { live: true, limit: 100 });
      runInAction(() => {
        if (revision !== this.revision) return;
        this.runs = page.items;
        this.counts = page.counts;
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

  async select(id: RunId) {
    this.selectedId = id;
    this.steps = [];
    this.graph = null;
    this.payload = null;
    await this.loadDetail(id);
  }

  async stop(id: RunId) {
    await this.act(id, () => this.ipc.call("runs.cancel", { id }));
  }

  async retry(id: RunId) {
    await this.act(id, async () => {
      await this.ipc.call("runs.retry", { id });
    });
  }

  async decide(id: RunId, approvalId: string, approve: boolean) {
    await this.act(id, async () => {
      if (approve) await this.ipc.call("runs.approve", { id, approvalId, always: false });
      else await this.ipc.call("runs.deny", { id, approvalId });
      runInAction(() => {
        const run = this.runs.find((item) => item.id === id);
        if (run) run.approval = undefined;
      });
    });
  }

  async clearFinished() {
    if (this.clearing) return;
    this.clearing = true;
    try {
      await this.ipc.call("runs.clearFinished", undefined);
      await this.load();
    } catch (error) {
      runInAction(() => {
        this.error = errorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.clearing = false;
      });
    }
  }

  dispose() {
    this.mounted = false;
    ++this.revision;
    this.unobserve?.();
    this.unobserve = undefined;
    this.seen.clear();
    if (this.ticker !== undefined) clearInterval(this.ticker);
    this.ticker = undefined;
    if (this.refreshTimer !== undefined) clearTimeout(this.refreshTimer);
    this.refreshTimer = undefined;
    this.loading = false;
  }

  receive(event: RoutedEvent) {
    const seen = this.seen.get(event.streamId);
    if (seen !== undefined && event.seq <= seen) return;
    this.seen.set(event.streamId, event.seq);
    const run = this.runs.find((item) => item.streamId === event.streamId);
    if (!run || event.gap !== undefined) {
      this.scheduleRefresh();
      if (!run) return;
    }
    if (event.type === "progress") run.progress = { done: event.done, total: event.total };
    if (event.type === "log") {
      const status = event.line.status;
      if (typeof status === "string" && status in this.counts.byStatus)
        run.status = status as RunDto["status"];
    }
    if (event.type === "approval") {
      const approval = RunApprovalDto.safeParse(event.request);
      if (approval.success) run.approval = approval.data;
    }
    if (event.type === "step") {
      const step = parseStepEvent(event.step);
      if (step) {
        run.activeNodeId = step.status === "running" ? step.nodeId : undefined;
        if (step.status === "succeeded")
          run.progress = {
            done: Math.min(run.progress.done + 1, run.progress.total),
            total: run.progress.total,
          };
        if (run.id === this.selectedId) this.mergeStep(step);
      }
    }
    if (event.type === "end") {
      run.activeNodeId = undefined;
      run.approval = undefined;
      this.seen.delete(event.streamId);
      this.scheduleRefresh();
    }
  }

  private mergeStep(step: StepDto) {
    const index = this.steps.findIndex((item) => item.id === step.id);
    if (index === -1) this.steps.push(step);
    else this.steps[index] = step;
  }

  private async loadDetail(id: RunId) {
    try {
      const detail = await this.ipc.call("runs.detail", { id });
      runInAction(() => {
        if (this.selectedId !== id) return;
        this.steps = detail.steps;
        this.graph = detail.run.graph;
        this.payload = detail.run.input;
      });
    } catch (error) {
      runInAction(() => {
        this.error = errorCopy(error);
      });
    }
  }

  private async act(id: RunId, action: () => Promise<unknown>) {
    if (this.busyId !== null) return;
    this.busyId = id;
    this.error = null;
    try {
      await action();
      await this.load();
    } catch (error) {
      runInAction(() => {
        this.error = errorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.busyId = null;
      });
    }
  }

  private scheduleRefresh() {
    if (!this.mounted || this.refreshTimer !== undefined) return;
    this.refreshTimer = setTimeout(() => {
      this.refreshTimer = undefined;
      void this.load();
      if (this.selectedId !== null) void this.loadDetail(this.selectedId);
    }, 200);
  }
}

function errorCopy(error: unknown): string {
  return error instanceof Error ? error.message : "Не удалось получить список задач.";
}
