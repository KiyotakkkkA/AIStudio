import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { BrowserSettingsDto, Contract } from "@zvs/shared";
import { BROWSER_SAVE_FAILED } from "./browserSettingsCopy";
import { CLEAR_DATA_DONE, CLEAR_DATA_FAILED } from "./browserDataCopy";

export const DEFAULT_BROWSER_SETTINGS: BrowserSettingsDto = {
  enabled: true,
  webLinks: "studio",
  localLinks: "studio",
  fullUrl: false,
  passwordAutosave: false,
  formAutofill: true,
  downloadDir: null,
  askDownload: false,
};

export default class BrowserSettingsStore {
  settings: BrowserSettingsDto = { ...DEFAULT_BROWSER_SETTINGS };
  loaded = false;
  saving = false;
  error: string | null = null;
  clearing = false;
  notice: string | null = null;

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

  async pickDownloadDir() {
    this.error = null;
    try {
      const settings = await this.ipc.call("browserSettings.pickDownloadDir", undefined);
      runInAction(() => {
        this.settings = settings;
      });
    } catch {
      runInAction(() => {
        this.error = BROWSER_SAVE_FAILED;
      });
    }
  }

  async clearData() {
    if (this.clearing) return;
    this.clearing = true;
    this.error = null;
    this.notice = null;
    try {
      await this.ipc.call("browserSettings.clearData", undefined);
      runInAction(() => {
        this.notice = CLEAR_DATA_DONE;
      });
    } catch {
      runInAction(() => {
        this.error = CLEAR_DATA_FAILED;
      });
    } finally {
      runInAction(() => {
        this.clearing = false;
      });
    }
  }

  dismissError() {
    this.error = null;
  }
}
