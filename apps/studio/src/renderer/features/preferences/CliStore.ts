import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { CliInstallDto, Contract } from "@zvs/shared";
import { CLI_NEW_TERMINAL, CLI_REMOVED, CLI_STATUS_FAILED, cliFailureCopy } from "./cliCopy";

export default class CliStore {
  status: CliInstallDto | null = null;
  busy = false;
  error: string | null = null;
  notice: string | null = null;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly onInstalled: () => Promise<void> = async () => {},
  ) {
    makeAutoObservable<CliStore, "ipc" | "onInstalled">(
      this,
      { ipc: false, onInstalled: false },
      { autoBind: true },
    );
  }

  get available() {
    return this.status?.available ?? false;
  }

  get installed() {
    return this.status?.installed ?? false;
  }

  async load() {
    try {
      const status = await this.ipc.call("cli.status", undefined);
      runInAction(() => {
        this.status = status;
      });
    } catch {
      runInAction(() => {
        this.error = CLI_STATUS_FAILED;
      });
    }
  }

  async install() {
    await this.change(true);
  }

  async uninstall() {
    await this.change(false);
  }

  private async change(installing: boolean) {
    if (this.busy) return;
    this.busy = true;
    this.error = null;
    this.notice = null;
    try {
      const status = await this.ipc.call(installing ? "cli.install" : "cli.uninstall", undefined);
      runInAction(() => {
        this.status = status;
        this.notice = installing ? CLI_NEW_TERMINAL : CLI_REMOVED;
      });
      if (installing) await this.onInstalled();
    } catch (error) {
      runInAction(() => {
        this.error = cliFailureCopy(error, installing);
      });
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }
}
