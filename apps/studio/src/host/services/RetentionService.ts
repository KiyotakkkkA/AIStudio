import type { UnitOfWork } from "../data/UnitOfWork.ts";
import type { Logger } from "../platform/logger.ts";
import type { SettingService } from "./SettingService.ts";

export const RETENTION_DAYS_KEY = "runs.retention.days";
export const DEFAULT_RETENTION_DAYS = 30;
const DAY_MS = 24 * 60 * 60 * 1000;

export interface RetentionServiceOptions {
  data: UnitOfWork;
  settings?: SettingService;
  logger?: Logger;
  intervalMs?: number;
  batch?: number;
  days?: number;
  clock?: () => number;
}

export class RetentionService {
  private timer: ReturnType<typeof setInterval> | undefined;
  private readonly clock: () => number;

  constructor(private readonly options: RetentionServiceOptions) {
    this.clock = options.clock ?? Date.now;
  }

  get days(): number {
    const stored = this.options.settings?.get(RETENTION_DAYS_KEY)?.value;
    if (typeof stored === "number" && Number.isFinite(stored) && stored > 0)
      return Math.floor(stored);
    return this.options.days ?? DEFAULT_RETENTION_DAYS;
  }

  start(): void {
    if (this.timer) return;
    this.timer = setInterval(() => this.sweep(), this.options.intervalMs ?? 6 * 60 * 60 * 1000);
    this.timer.unref?.();
    this.sweep();
  }

  stop(): void {
    if (!this.timer) return;
    clearInterval(this.timer);
    this.timer = undefined;
  }

  sweep(): number {
    const before = this.clock() - this.days * DAY_MS;
    const batch = this.options.batch ?? 200;
    try {
      const pruned = this.options.data.transaction(({ runs }) => {
        const ids = runs.prunable(before, batch);
        runs.dropPayloads(ids, this.clock());
        return ids.length;
      });
      if (pruned > 0)
        this.options.logger?.log("info", "runs", "Pruned step payloads past retention", {
          runs: pruned,
          days: this.days,
        });
      return pruned;
    } catch (error: unknown) {
      this.options.logger?.log("error", "runs", "Retention sweep failed", {
        error: String(error),
      });
      return 0;
    }
  }
}
