import { timestampNow } from "@zvs/shared";
import type { EventBus } from "../platform/events.ts";
import type { Logger } from "../platform/logger.ts";
import type { ProviderService } from "./ProviderService.ts";
import type { SettingService } from "./SettingService.ts";

export const HEALTH_CHECK_ENABLED_KEY = "providers.healthcheck.enabled";
export const HEALTH_CHECK_INTERVAL_KEY = "providers.healthcheck.minutes";
export const DEFAULT_HEALTH_CHECK_INTERVAL_MINUTES = 15;
export const MIN_HEALTH_CHECK_INTERVAL_MINUTES = 1;

export const HEALTH_CHECK_ENABLED_BY_DEFAULT = false;

export interface Scheduler {
  setInterval(handler: () => void, ms: number): unknown;
  clearInterval(handle: unknown): void;
}

export interface HealthCheckServiceOptions {
  providers: ProviderService;
  settings: SettingService;
  logger?: Logger;
  events?: EventBus;
  clock?: () => number;
  scheduler?: Scheduler;
}

const systemScheduler: Scheduler = {
  setInterval: (handler, ms) => {
    const handle = setInterval(handler, ms);
    (handle as { unref?: () => void }).unref?.();
    return handle;
  },
  clearInterval: (handle) => clearInterval(handle as ReturnType<typeof setInterval>),
};

export class HealthCheckService {
  readonly #providers: ProviderService;
  readonly #settings: SettingService;
  readonly #logger: Logger | undefined;
  readonly #events: EventBus | undefined;
  readonly #clock: () => number;
  readonly #scheduler: Scheduler;
  #handle: unknown = null;
  #sweep: Promise<void> | null = null;
  #generation = 0;
  #controller: AbortController | null = null;

  constructor(options: HealthCheckServiceOptions) {
    this.#providers = options.providers;
    this.#settings = options.settings;
    this.#logger = options.logger;
    this.#events = options.events;
    this.#clock = options.clock ?? Date.now;
    this.#scheduler = options.scheduler ?? systemScheduler;
  }

  get running(): boolean {
    return this.#handle !== null;
  }

  get enabled(): boolean {
    const stored = this.#settings.get(HEALTH_CHECK_ENABLED_KEY)?.value;
    return typeof stored === "boolean" ? stored : HEALTH_CHECK_ENABLED_BY_DEFAULT;
  }

  get intervalMinutes(): number {
    const stored = this.#settings.get(HEALTH_CHECK_INTERVAL_KEY)?.value;
    if (typeof stored !== "number" || !Number.isFinite(stored)) {
      return DEFAULT_HEALTH_CHECK_INTERVAL_MINUTES;
    }
    return Math.max(MIN_HEALTH_CHECK_INTERVAL_MINUTES, Math.round(stored));
  }

  start(): boolean {
    if (this.running) return true;
    if (!this.enabled) {
      this.#logger?.log("info", "providers", "Background health check stays off", {
        setting: HEALTH_CHECK_ENABLED_KEY,
      });
      return false;
    }
    const minutes = this.intervalMinutes;
    this.#handle = this.#scheduler.setInterval(() => void this.runOnce(), minutes * 60 * 1000);
    this.#logger?.log("info", "providers", "Started the background health check", { minutes });
    return true;
  }

  stop(): void {
    this.#generation += 1;
    this.#controller?.abort();
    if (this.#handle === null) return;
    this.#scheduler.clearInterval(this.#handle);
    this.#handle = null;
    this.#logger?.log("info", "providers", "Stopped the background health check");
  }

  async dispose(): Promise<void> {
    this.stop();
    await this.#sweep;
  }

  runOnce(): Promise<void> {
    if (this.#sweep !== null) return this.#sweep;
    const sweep = this.#sweep0().finally(() => {
      this.#sweep = null;
      this.#controller = null;
    });
    this.#sweep = sweep;
    return sweep;
  }

  async #sweep0(): Promise<void> {
    if (!this.enabled) return;
    const generation = this.#generation;
    const controller = new AbortController();
    this.#controller = controller;
    const targets = this.#providers
      .list({ enabled: true })
      .filter((provider) => !this.#providers.isProbing(provider.id));
    if (targets.length === 0) return;
    const stream = this.#events?.openStream();
    let done = 0;
    try {
      for (const provider of targets) {
        if (generation !== this.#generation || !this.enabled) break;
        if (this.#providers.isProbing(provider.id)) continue;
        await this.#providers.probe({ id: provider.id }, controller.signal);
        done += 1;
        stream?.emit({ type: "progress", done, total: targets.length });
      }
      stream?.end({ status: "ok" });
    } catch (error: unknown) {
      this.#logger?.log("warn", "providers", "Health check sweep stopped early", {
        done,
        total: targets.length,
        failed: error !== undefined,
      });
      stream?.end({ status: "failed" });
    }
    this.#logger?.log("debug", "providers", "Health check sweep finished", {
      done,
      at: timestampNow(this.#clock),
    });
  }
}
