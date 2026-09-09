import { makeAutoObservable } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract } from "@zvs/shared";
import type { EventRouter } from "../app/EventRouter";
import { SecretStore } from "../features/secrets/SecretStore";
import { UiStore } from "./UiStore";

export interface RootStoreEnvironment {
  readonly ipc: IpcClient<Contract>;
  readonly events: EventRouter;
}

export class RootStore {
  readonly ui: UiStore;
  readonly secrets: SecretStore;

  constructor(private readonly environment: RootStoreEnvironment) {
    this.ui = new UiStore(environment.ipc);
    this.secrets = new SecretStore(environment.ipc);
    makeAutoObservable<RootStore, "environment">(
      this,
      { environment: false, ui: false, secrets: false },
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
    this.environment.events.dispose();
  }
}

export function createRootStore(environment: RootStoreEnvironment): RootStore {
  return new RootStore(environment);
}

export default RootStore;
