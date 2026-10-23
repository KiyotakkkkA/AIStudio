import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract } from "@zvs/shared";
import { ARTIFACT_SAVED, artifactErrorCopy } from "./artifacts";

export type ArtifactAction = "save" | "reveal" | "open";

export default class ArtifactActions {
  busy: Record<string, ArtifactAction> = {};
  errors: Record<string, string> = {};
  notices: Record<string, string> = {};
  applications: Record<string, string | null> = {};

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<ArtifactActions, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  save(id: string) {
    return this.act(id, "save", async () => {
      const result = await this.ipc.call("artifacts.save", { id });
      return result.path === undefined ? null : ARTIFACT_SAVED;
    });
  }

  open(id: string) {
    return this.act(id, "open", async () => {
      await this.ipc.call("artifacts.open", { id });
      return null;
    });
  }

  async loadApplication(id: string, format: string): Promise<void> {
    const key = format.toLowerCase();
    if (key in this.applications) return;
    this.applications[key] = null;
    try {
      const application = await this.ipc.call("artifacts.application", { id });
      runInAction(() => {
        this.applications[key] = application.name ?? null;
      });
    } catch {
      runInAction(() => {
        delete this.applications[key];
      });
    }
  }

  applicationOf(format: string): string | null {
    return this.applications[format.toLowerCase()] ?? null;
  }

  reveal(id: string) {
    return this.act(id, "reveal", async () => {
      await this.ipc.call("artifacts.reveal", { id });
      return null;
    });
  }

  private async act(
    id: string,
    action: ArtifactAction,
    call: () => Promise<string | null>,
  ): Promise<boolean> {
    if (this.busy[id] !== undefined) return false;
    this.busy[id] = action;
    delete this.errors[id];
    delete this.notices[id];
    try {
      const notice = await call();
      runInAction(() => {
        if (notice !== null) this.notices[id] = notice;
      });
      return true;
    } catch (error) {
      runInAction(() => {
        this.errors[id] = artifactErrorCopy(error);
      });
      return false;
    } finally {
      runInAction(() => {
        delete this.busy[id];
      });
    }
  }
}
