import type { Contract } from "@zvs/shared";
import type { ChatService } from "../services/ChatService.ts";
import { createChatHandlers } from "./chat.ts";
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
import type { VectorEvalService } from "../services/VectorEvalService.ts";
import { createVectorStoreHandlers, type SourcePicker } from "./vectorStores.ts";
import type { GraphService } from "../services/GraphService.ts";
import { createGraphRelationHandlers } from "./graphRelations.ts";
import type { IndexingService } from "../indexing/IndexingService.ts";
import type { DownloadService } from "../downloads/DownloadService.ts";
import { createDownloadHandlers } from "./downloads.ts";
import type { ToolRegistry } from "../tools/ToolRegistry.ts";
import { createToolHandlers } from "./tools.ts";
import type { McpService } from "../services/McpService.ts";
import { createMcpHandlers } from "./mcp.ts";
import type { RuntimeService } from "../runtimes/RuntimeService.ts";
import { createRuntimeHandlers } from "./runtimes.ts";

export interface HostIpcDependencies {
  chat: ChatService;
  runs: RunService;
  vectorStores: VectorStoreService;
  vectorEval?: VectorEvalService;
  graph: GraphService;
  indexing?: IndexingService;
  downloads?: DownloadService;
  runtimes?: RuntimeService;
  tools?: ToolRegistry;
  mcp?: McpService;
  pickSources?: SourcePicker;
  system: SystemService;
  accounts: AccountService;
  settings: SettingService;
  secrets: SecretService;
  providers: ProviderService;
  healthCheck?: HealthCheckService;
  /** Mirrors the saved `ui.theme` into the platform colour scheme. */
  applyTheme?: (value: unknown) => void;
  events?: EventBus;
  clock?: () => number;
  intervalMs?: number;
}

export function createHandlers(dependencies: HostIpcDependencies): IpcHandlers<Contract> {
  return {
    ...createChatHandlers(dependencies.chat),
    ...createRunHandlers(dependencies.runs),
    ...createVectorStoreHandlers({
      service: dependencies.vectorStores,
      indexing: dependencies.indexing,
      runs: dependencies.runs,
      evaluation: dependencies.vectorEval,
      pick: dependencies.pickSources,
    }),
    ...createGraphRelationHandlers(dependencies.graph),
    ...createDownloadHandlers(dependencies.downloads),
    ...createRuntimeHandlers(dependencies.runtimes),
    ...createToolHandlers(dependencies.tools),
    ...createMcpHandlers(dependencies.mcp),
    ...createSystemHandlers({
      system: dependencies.system,
      runs: dependencies.runs,
      events: dependencies.events ?? createEventBus(),
      clock: dependencies.clock,
      intervalMs: dependencies.intervalMs,
    }),
    ...createSettingHandlers(
      dependencies.settings,
      dependencies.healthCheck,
      dependencies.applyTheme,
    ),
    ...createSecretHandlers(dependencies.secrets),
    ...createProviderHandlers(dependencies.providers),
    ...createAccountHandlers(dependencies.accounts),
  };
}
