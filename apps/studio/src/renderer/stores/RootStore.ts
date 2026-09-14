import { makeAutoObservable } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract } from "@zvs/shared";
import type { EventRouter } from "../app/EventRouter";
import { AccountStore } from "../features/providers/AccountStore";
import { ProviderStore } from "../features/providers/ProviderStore";
import { SecretStore } from "../features/secrets/SecretStore";
import { UiStore } from "./UiStore";
import VectorStoreStore from "../features/vector-stores/VectorStoreStore";
import ChatStore from "../features/chat/ChatStore";
import DownloadStore from "../features/downloads/DownloadStore";
import TaskStore from "../features/tasks/TaskStore";
import RunHistoryStore from "../features/tasks/RunHistoryStore";

export interface RootStoreEnvironment {
  readonly ipc: IpcClient<Contract>;
  readonly events: EventRouter;
}

export class RootStore {
  readonly ui: UiStore;
  readonly secrets: SecretStore;
  readonly providers: ProviderStore;
  readonly accounts: AccountStore;
  readonly vectorStores: VectorStoreStore;
  readonly chat: ChatStore;
  readonly tasks: TaskStore;
  readonly downloads: DownloadStore;
  readonly runs: RunHistoryStore;

  constructor(private readonly environment: RootStoreEnvironment) {
    this.ui = new UiStore(environment.ipc);
    this.secrets = new SecretStore(environment.ipc);
    this.providers = new ProviderStore(environment.ipc);
    this.accounts = new AccountStore(environment);
    this.vectorStores = new VectorStoreStore(environment.ipc, environment.events);
    this.chat = new ChatStore(environment.ipc, environment.events);
    this.tasks = new TaskStore(environment.ipc, environment.events);
    this.downloads = new DownloadStore(environment.ipc, environment.events);
    this.runs = new RunHistoryStore(environment.ipc);
    makeAutoObservable<RootStore, "environment">(
      this,
      {
        environment: false,
        ui: false,
        secrets: false,
        providers: false,
        accounts: false,
        vectorStores: false,
        chat: false,
        tasks: false,
        downloads: false,
        runs: false,
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
    await this.ui.restore();
  }

  dispose(): void {
    this.accounts.dispose();
    this.vectorStores.dispose();
    this.chat.dispose();
    this.tasks.dispose();
    this.downloads.dispose();
    this.runs.dispose();
    this.environment.events.dispose();
  }
}

export function createRootStore(environment: RootStoreEnvironment): RootStore {
  return new RootStore(environment);
}
