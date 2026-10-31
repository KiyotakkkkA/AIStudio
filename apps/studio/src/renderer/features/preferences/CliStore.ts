import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { CliInstallDto, CliTheme, Contract } from "@zvs/shared";
import {
  CLI_NEW_TERMINAL,
  CLI_REMOVED,
  CLI_STATUS_FAILED,
  CLI_THEME_LOAD_FAILED,
  CLI_THEME_SAVE_FAILED,
  cliFailureCopy,
} from "./cliCopy";

export default class CliStore {
  status: CliInstallDto | null = null;
  busy = false;
  error: string | null = null;
  notice: string | null = null;
  theme: CliTheme | null = null;
  themeError: string | null = null;
  private themeRequest = 0;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly onInstalled: () => Promise<void> = async () => {},
  ) {
    makeAutoObservable<CliStore, "ipc" | "onInstalled" | "themeRequest">(
      this,
      { ipc: false, onInstalled: false, themeRequest: false },
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
    await Promise.all([this.loadStatus(), this.loadTheme()]);
  }

  async setTheme(theme: CliTheme) {
    if (theme === this.theme) return;
    const previous = this.theme;
    const request = ++this.themeRequest;
    this.theme = theme;
    this.themeError = null;
    try {
      const saved = await this.ipc.call("cli.theme.set", { theme });
      runInAction(() => {
        if (request === this.themeRequest) this.theme = saved.theme;
      });
    } catch {
      runInAction(() => {
        if (request !== this.themeRequest) return;
        this.theme = previous;
        this.themeError = CLI_THEME_SAVE_FAILED;
      });
    }
  }

  private async loadTheme() {
    const request = this.themeRequest;
    try {
      const { theme } = await this.ipc.call("cli.theme.get", undefined);
      runInAction(() => {
        if (request === this.themeRequest) this.theme = theme;
      });
    } catch {
      runInAction(() => {
        if (request === this.themeRequest) this.themeError = CLI_THEME_LOAD_FAILED;
      });
    }
  }

  private async loadStatus() {
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
