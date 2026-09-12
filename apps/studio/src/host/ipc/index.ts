import type { Contract } from "@zvs/shared";
import type { RunService } from "../services/RunService.ts";
import { createRunHandlers } from "./runs.ts";
import type { IpcHandlers } from "@zvs/ipc";
import { createEventBus, type EventBus } from "../platform/events.ts";
import type { SecretService } from "../services/SecretService.ts";
import type { SettingService } from "../services/SettingService.ts";
import type { ProviderService } from "../services/ProviderService.ts";
import type { HealthCheckService } from "../services/HealthCheckService.ts";
import { createSystemHandlers } from "./system.ts";
import { createSecretHandlers } from "./secrets.ts";
import { createSettingHandlers } from "./settings.ts";
import { createProviderHandlers } from "./providers.ts";
import { createAccountHandlers } from "./accounts.ts";
import type { AccountService } from "../services/AccountService.ts";
import type { SystemService } from "../services/SystemService.ts";
import type { VectorStoreService } from "../services/VectorStoreService.ts";
import { createVectorStoreHandlers } from "./vectorStores.ts";

export interface HostIpcDependencies {
  runs: RunService;
  vectorStores: VectorStoreService;
  system: SystemService;
  accounts: AccountService;
  settings: SettingService;
  secrets: SecretService;
  providers: ProviderService;
  healthCheck?: HealthCheckService;
  events?: EventBus;
  clock?: () => number;
  intervalMs?: number;
}

export function createHandlers(dependencies: HostIpcDependencies): IpcHandlers<Contract> {
  return {
    ...createRunHandlers(dependencies.runs),
    ...createVectorStoreHandlers(dependencies.vectorStores),
    ...createSystemHandlers({
      system: dependencies.system,
      events: dependencies.events ?? createEventBus(),
      clock: dependencies.clock,
      intervalMs: dependencies.intervalMs,
    }),
    ...createSettingHandlers(dependencies.settings, dependencies.healthCheck),
    ...createSecretHandlers(dependencies.secrets),
    ...createProviderHandlers(dependencies.providers),
    ...createAccountHandlers(dependencies.accounts),
  };
}
