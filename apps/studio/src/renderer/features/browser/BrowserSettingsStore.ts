import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { BrowserSettingsDto, Contract } from "@zvs/shared";
import { BROWSER_SAVE_FAILED } from "./browserSettingsCopy";

export const DEFAULT_BROWSER_SETTINGS: BrowserSettingsDto = {
  enabled: true,
  webLinks: "studio",
  localLinks: "studio",
  fullUrl: false,
};

export default class BrowserSettingsStore {
  settings: BrowserSettingsDto = { ...DEFAULT_BROWSER_SETTINGS };
  loaded = false;
  saving = false;
  error: string | null = null;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<BrowserSettingsStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  get enabled(): boolean {
    return this.settings.enabled;
  }

  async load() {
    try {
      const settings = await this.ipc.call("browserSettings.get", undefined);
      runInAction(() => {
        if (!this.saving) this.settings = settings;
        this.loaded = true;
      });
    } catch {
      runInAction(() => {
        this.loaded = true;
      });
    }
  }

  async update(patch: Partial<BrowserSettingsDto>) {
    const previous = this.settings;
    const next = { ...previous, ...patch };
    this.settings = next;
    this.saving = true;
    this.error = null;
    try {
      const saved = await this.ipc.call("browserSettings.set", next);
      runInAction(() => {
        this.settings = saved;
      });
    } catch {
      runInAction(() => {
        this.settings = previous;
        this.error = BROWSER_SAVE_FAILED;
      });
    } finally {
      runInAction(() => {
        this.saving = false;
      });
    }
  }

  dismissError() {
    this.error = null;
  }
}
