import type { Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { SettingService } from "../services/SettingService.ts";

export type SettingHandlers = Pick<IpcHandlers<Contract>, "settings.get" | "settings.set">;

export function createSettingHandlers(settings: SettingService): SettingHandlers {
  return {
    "settings.get": ({ key }) => settings.get(key) ?? { key },
    "settings.set": ({ key, value }) => settings.set(key, value),
  };
}
