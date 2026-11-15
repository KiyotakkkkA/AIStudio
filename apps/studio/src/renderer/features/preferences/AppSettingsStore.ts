import { makeAutoObservable, runInAction } from "mobx";
import { isIpcError, type IpcClient } from "@zvs/ipc";
import { AppErrorCode, type Contract, type NotificationTargetDto } from "@zvs/shared";
import { AUTOSTART_FAILED, SETTING_SAVE_FAILED } from "./preferencesPresentation";

const APP_SETTING_KEYS = [
  "app.autostart",
  "app.tray",
  "notifications.enabled",
  "notifications.approval",
  "notifications.turn",
  "notifications.download",
  "computer.enabled",
  "memory.enabled",
  "memory.model",
  "cli.confirmLoosening",
] as const;
export type AppSettingKey = (typeof APP_SETTING_KEYS)[number];

const APP_SETTING_DEFAULTS: Record<AppSettingKey, boolean> = {
  "app.autostart": false,
  "app.tray": false,
  "notifications.enabled": true,
  "notifications.approval": true,
  "notifications.turn": true,
  "notifications.download": true,
  "computer.enabled": true,
  "memory.enabled": true,
  "memory.model": true,
  "cli.confirmLoosening": true,
};

const APP_SETTING_DEPENDENTS: Partial<Record<AppSettingKey, readonly AppSettingKey[]>> = {
  "memory.enabled": ["memory.model"],
  "notifications.enabled": [
    "notifications.approval",
    "notifications.turn",
    "notifications.download",
  ],
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
    for (const parent of APP_SETTING_KEYS) {
      if (this.values[parent]) continue;
      for (const dependent of APP_SETTING_DEPENDENTS[parent] ?? []) {
        await this.set(dependent, false);
      }
    }
  }

  async takePendingTarget(): Promise<NotificationTargetDto | null> {
    try {
      return await this.ipc.call("notifications.pending", undefined);
    } catch {
      return null;
    }
  }

  async set(key: AppSettingKey, value: boolean) {
    const parent = APP_SETTING_KEYS.find((candidate) =>
      APP_SETTING_DEPENDENTS[candidate]?.includes(key),
    );
    if (
      parent !== undefined &&
      (this.pending[parent] !== undefined || (value && !this.values[parent]))
    )
      return;
    if (this.values[key] === value || this.pending[key] !== undefined) return;
    const dependents = value ? [] : (APP_SETTING_DEPENDENTS[key] ?? []);
    const changes: readonly [AppSettingKey, boolean][] = [
      ...dependents
        .filter((dependent) => this.values[dependent])
        .map((dependent): [AppSettingKey, boolean] => [dependent, false]),
      [key, value],
    ];
    if (changes.some(([changed]) => this.pending[changed] !== undefined)) return;
    const previous = { ...this.values };
    for (const [changed, next] of changes) {
      this.values[changed] = next;
      this.pending[changed] = next;
      delete this.errors[changed];
    }
    try {
      for (const [index, [changed, next]] of changes.entries()) {
        try {
          await this.ipc.call("settings.set", { key: changed, value: next });
        } catch (error) {
          runInAction(() => {
            for (const [unsaved] of changes.slice(index)) this.values[unsaved] = previous[unsaved];
            this.errors[changed] = failureCopy(changed, error);
          });
          break;
        }
      }
    } finally {
      runInAction(() => {
        for (const [changed] of changes) delete this.pending[changed];
      });
    }
  }
}
