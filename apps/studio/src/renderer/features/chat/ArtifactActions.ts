import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract } from "@zvs/shared";
import { ARTIFACT_SAVED, artifactErrorCopy } from "./artifacts";

export type ArtifactAction = "save" | "reveal";

export default class ArtifactActions {
  busy: Record<string, ArtifactAction> = {};
  errors: Record<string, string> = {};
  notices: Record<string, string> = {};

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<ArtifactActions, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  save(id: string) {
    return this.act(id, "save", async () => {
      const result = await this.ipc.call("artifacts.save", { id });
      return result.path === undefined ? null : ARTIFACT_SAVED;
    });
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
