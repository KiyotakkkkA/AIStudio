import type { Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import { createEventBus, type EventBus } from "../platform/events.ts";
import type { SecretService } from "../services/SecretService.ts";
import type { SettingService } from "../services/SettingService.ts";
import { createSystemHandlers } from "./system.ts";
import { createSecretHandlers } from "./secrets.ts";
import { createSettingHandlers } from "./settings.ts";

export interface HostIpcDependencies {
  settings: SettingService;
  secrets: SecretService;
  events?: EventBus;
  clock?: () => number;
  intervalMs?: number;
}

export function createHandlers(dependencies: HostIpcDependencies): IpcHandlers<Contract> {
  return {
    ...createSystemHandlers({
      events: dependencies.events ?? createEventBus(),
      clock: dependencies.clock,
      intervalMs: dependencies.intervalMs,
    }),
    ...createSettingHandlers(dependencies.settings),
    ...createSecretHandlers(dependencies.secrets),
  };
}
