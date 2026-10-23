import { makeAutoObservable, runInAction } from "mobx";
import { isIpcError, type IpcClient } from "@zvs/ipc";
import { AppErrorCode, type Contract } from "@zvs/shared";
import { AUTOSTART_FAILED, SETTING_SAVE_FAILED } from "./preferencesPresentation";

export const APP_SETTING_KEYS = [
  "app.autostart",
  "app.tray",
  "notifications.enabled",
  "notifications.approval",
  "notifications.turn",
  "notifications.download",
] as const;
export type AppSettingKey = (typeof APP_SETTING_KEYS)[number];

export const APP_SETTING_DEFAULTS: Record<AppSettingKey, boolean> = {
  "app.autostart": false,
  "app.tray": false,
  "notifications.enabled": true,
  "notifications.approval": true,
  "notifications.turn": true,
  "notifications.download": true,
};

function failureCopy(key: AppSettingKey, error: unknown): string {
  if (key === "app.autostart" && isIpcError(error) && error.code === AppErrorCode.NATIVE_ERROR)
    return AUTOSTART_FAILED;
  return SETTING_SAVE_FAILED;
}

export default class AppSettingsStore {
  values: Record<AppSettingKey, boolean> = { ...APP_SETTING_DEFAULTS };
  errors: Partial<Record<AppSettingKey, string>> = {};
  pending: Partial<Record<AppSettingKey, boolean>> = {};
  loaded = false;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<AppSettingsStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  async load() {
    const stored = await Promise.all(
      APP_SETTING_KEYS.map(async (key) => {
        try {
          return [key, (await this.ipc.call("settings.get", { key })).value] as const;
        } catch {
          return [key, undefined] as const;
        }
      }),
    );
    runInAction(() => {
      for (const [key, value] of stored) {
        if (typeof value === "boolean" && this.pending[key] === undefined) this.values[key] = value;
      }
      this.loaded = true;
    });
  }

  async set(key: AppSettingKey, value: boolean) {
    if (this.values[key] === value || this.pending[key] !== undefined) return;
    const previous = this.values[key];
    this.values[key] = value;
    this.pending[key] = value;
    delete this.errors[key];
    try {
      await this.ipc.call("settings.set", { key, value });
    } catch (error) {
      runInAction(() => {
        this.values[key] = previous;
        this.errors[key] = failureCopy(key, error);
      });
    } finally {
      runInAction(() => {
        delete this.pending[key];
      });
    }
  }
}
