import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract, ToolchainDto, ToolchainRuntime } from "@zvs/shared";
import type { EventRouter, RoutedEvent } from "../../app/EventRouter";
import { TOOLCHAIN_ORDER, toolchainErrorCopy, type ToolchainAction } from "./toolchainPresentation";

export default class ToolchainStore {
  runtimes: ToolchainDto[] = [];
  loading = false;
  loaded = false;
  error: string | null = null;
  busy: Partial<Record<ToolchainRuntime, ToolchainAction>> = {};
  errors: Partial<Record<ToolchainRuntime, string>> = {};
  private unobserve: (() => void) | undefined;
  private refreshTimer: ReturnType<typeof setTimeout> | undefined;
  private mounted = false;
  private revision = 0;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly events: EventRouter,
  ) {
    makeAutoObservable<
      ToolchainStore,
      "ipc" | "events" | "unobserve" | "refreshTimer" | "mounted" | "revision"
    >(
      this,
      {
        ipc: false,
        events: false,
        unobserve: false,
        refreshTimer: false,
        mounted: false,
        revision: false,
      },
      { autoBind: true },
    );
  }

  get ordered(): ToolchainDto[] {
    return TOOLCHAIN_ORDER.flatMap((runtime) => {
      const dto = this.runtimes.find((row) => row.runtime === runtime);
      return dto === undefined ? [] : [dto];
    });
  }

  get installing(): boolean {
    return this.runtimes.some((row) => row.status === "installing");
  }

  async mount() {
    if (this.mounted) return;
    this.mounted = true;
    this.unobserve = this.events.observe(this.receive);
    await this.load(false);
    if (this.runtimes.some((row) => row.source === "managed")) await this.load(true);
  }

  async load(checkUpdates = false) {
    const revision = ++this.revision;
    this.loading = !this.loaded;
    try {
      const runtimes = await this.ipc.call("toolchain.list", { checkUpdates });
      runInAction(() => {
        if (revision !== this.revision) return;
        this.runtimes = runtimes;
        this.error = null;
      });
    } catch (error) {
      runInAction(() => {
        if (revision === this.revision) this.error = toolchainErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        if (revision !== this.revision) return;
        this.loading = false;
        this.loaded = true;
      });
    }
  }

  install(runtime: ToolchainRuntime) {
    return this.act(runtime, "install", () => this.ipc.call("toolchain.install", { runtime }));
  }

  update(runtime: ToolchainRuntime) {
    return this.act(runtime, "update", () => this.ipc.call("toolchain.update", { runtime }));
  }

  scan(runtime: ToolchainRuntime) {
    return this.act(runtime, "scan", () => this.ipc.call("toolchain.scan", { runtime }));
  }

  select(runtime: ToolchainRuntime) {
    return this.act(runtime, "select", () => this.ipc.call("toolchain.select", { runtime }));
  }

  remove(runtime: ToolchainRuntime) {
    return this.act(runtime, "remove", () => this.ipc.call("toolchain.remove", { runtime }));
  }

  receive(event: RoutedEvent) {
    if (event.type === "end" && this.installing) this.scheduleRefresh();
  }

  dispose() {
    this.mounted = false;
    ++this.revision;
    this.unobserve?.();
    this.unobserve = undefined;
    if (this.refreshTimer !== undefined) clearTimeout(this.refreshTimer);
    this.refreshTimer = undefined;
    this.loading = false;
  }

  private async act(
    runtime: ToolchainRuntime,
    action: ToolchainAction,
    call: () => Promise<ToolchainDto>,
  ): Promise<boolean> {
    if (this.busy[runtime] !== undefined) return false;
    this.busy[runtime] = action;
    delete this.errors[runtime];
    try {
      const dto = await call();
      runInAction(() => {
        ++this.revision;
        this.runtimes = this.runtimes.some((row) => row.runtime === dto.runtime)
          ? this.runtimes.map((row) => (row.runtime === dto.runtime ? dto : row))
          : [...this.runtimes, dto];
      });
      return true;
    } catch (error) {
      runInAction(() => {
        this.errors[runtime] = toolchainErrorCopy(error);
      });
      return false;
    } finally {
      runInAction(() => {
        delete this.busy[runtime];
      });
    }
  }

  private scheduleRefresh() {
    if (!this.mounted || this.refreshTimer !== undefined) return;
    this.refreshTimer = setTimeout(() => {
      this.refreshTimer = undefined;
      void this.load(false);
    }, 250);
  }
}
