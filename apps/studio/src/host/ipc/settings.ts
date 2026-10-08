import type { Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { SettingService } from "../services/SettingService.ts";
import {
  HEALTH_CHECK_ENABLED_KEY,
  HEALTH_CHECK_INTERVAL_KEY,
  type HealthCheckService,
} from "../services/HealthCheckService.ts";
import { THEME_SETTING_KEY } from "../platform/theme.ts";

export type SettingHandlers = Pick<IpcHandlers<Contract>, "settings.get" | "settings.set">;

export function createSettingHandlers(
  settings: SettingService,
  healthCheck?: HealthCheckService,
  applyTheme?: (value: unknown) => void,
): SettingHandlers {
  return {
    "settings.get": ({ key }) => settings.get(key) ?? { key },
    "settings.set": ({ key, value }) => {
      const result = settings.set(key, value);
      if (key === HEALTH_CHECK_ENABLED_KEY || key === HEALTH_CHECK_INTERVAL_KEY) {
        healthCheck?.stop();
        healthCheck?.start();
      }
      if (key === THEME_SETTING_KEY) applyTheme?.(value);
      return result;
    },
  };
}
