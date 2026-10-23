import { makeAutoObservable } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract } from "@zvs/shared";
import type { EventRouter } from "../app/EventRouter";
import { AccountStore } from "../features/providers/AccountStore";
import { ProviderStore } from "../features/providers/ProviderStore";
import { SecretStore } from "../features/secrets/SecretStore";
import { UiStore } from "./UiStore";
import ViewerStore from "./ViewerStore";
import VectorStoreStore from "../features/vector-stores/VectorStoreStore";
import ChatStore from "../features/chat/ChatStore";
import DownloadStore from "../features/downloads/DownloadStore";
import TaskStore from "../features/tasks/TaskStore";
import RunHistoryStore from "../features/tasks/RunHistoryStore";
import ConnectionStore from "../features/connections/ConnectionStore";
import ToolStore from "../features/tools/ToolStore";
import ToolchainStore from "../features/toolchain/ToolchainStore";
import SkillStore from "../features/skills/SkillStore";
import ScenarioStore from "../features/scenarios/ScenarioStore";
import PermissionsStore from "../features/permissions/PermissionsStore";
import AboutStore from "../features/preferences/AboutStore";
import AppSettingsStore from "../features/preferences/AppSettingsStore";

export interface RootStoreEnvironment {
  readonly ipc: IpcClient<Contract>;
  readonly events: EventRouter;
}

export class RootStore {
  readonly ui: UiStore;
  readonly viewer: ViewerStore;
  readonly secrets: SecretStore;
  readonly providers: ProviderStore;
  readonly accounts: AccountStore;
  readonly vectorStores: VectorStoreStore;
  readonly chat: ChatStore;
  readonly tasks: TaskStore;
  readonly downloads: DownloadStore;
  readonly runs: RunHistoryStore;
  readonly connections: ConnectionStore;
  readonly tools: ToolStore;
  readonly toolchain: ToolchainStore;
  readonly skills: SkillStore;
  readonly scenarios: ScenarioStore;
  readonly permissions: PermissionsStore;
  readonly about: AboutStore;
  readonly appSettings: AppSettingsStore;

  constructor(private readonly environment: RootStoreEnvironment) {
    this.ui = new UiStore(environment.ipc);
    this.secrets = new SecretStore(environment.ipc);
    this.providers = new ProviderStore(environment.ipc);
    this.accounts = new AccountStore(environment);
    this.vectorStores = new VectorStoreStore(environment.ipc, environment.events);
    this.chat = new ChatStore(environment.ipc, environment.events);
    this.viewer = new ViewerStore(environment.ipc, {
      attachSelection: (selection) => this.chat.attachSelection(selection),
    });
    this.tasks = new TaskStore(environment.ipc, environment.events);
    this.downloads = new DownloadStore(environment.ipc, environment.events);
    this.runs = new RunHistoryStore(environment.ipc);
    this.connections = new ConnectionStore(environment.ipc);
    this.tools = new ToolStore(environment.ipc);
    this.toolchain = new ToolchainStore(environment.ipc, environment.events);
    this.skills = new SkillStore(environment.ipc);
    this.scenarios = new ScenarioStore(environment.ipc);
    this.permissions = new PermissionsStore(environment.ipc);
    this.about = new AboutStore(environment.ipc);
    this.appSettings = new AppSettingsStore(environment.ipc);
    makeAutoObservable<RootStore, "environment">(
      this,
      {
        environment: false,
        ui: false,
        viewer: false,
        secrets: false,
        providers: false,
        accounts: false,
        vectorStores: false,
        chat: false,
        tasks: false,
        downloads: false,
        runs: false,
        connections: false,
        tools: false,
        toolchain: false,
        skills: false,
        scenarios: false,
        permissions: false,
        about: false,
        appSettings: false,
      },
      { autoBind: true },
    );
  }

  get ipc(): IpcClient<Contract> {
    return this.environment.ipc;
  }

  get events(): EventRouter {
    return this.environment.events;
  }

  async boot(): Promise<void> {
    await Promise.all([this.ui.restore(), this.viewer.restore()]);
  }

  dispose(): void {
    this.viewer.dispose();
    this.accounts.dispose();
    this.vectorStores.dispose();
    this.chat.dispose();
    this.tasks.dispose();
    this.downloads.dispose();
    this.runs.dispose();
    this.connections.dispose();
    this.tools.dispose();
    this.toolchain.dispose();
    this.skills.dispose();
    this.scenarios.dispose();
    this.permissions.dispose();
    this.environment.events.dispose();
  }
}

export function createRootStore(environment: RootStoreEnvironment): RootStore {
  return new RootStore(environment);
}
