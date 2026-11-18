import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { ChatActivityDto, Contract } from "@zvs/shared";

export default class ActivityStore {
  activity: ChatActivityDto | null = null;
  loading = false;
  failed = false;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<ActivityStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  async load() {
    if (this.loading) return;
    this.loading = true;
    this.failed = false;
    try {
      const activity = await this.ipc.call("chat.activity", undefined);
      runInAction(() => {
        this.activity = activity;
      });
    } catch {
      runInAction(() => {
        this.failed = true;
      });
    } finally {
      runInAction(() => {
        this.loading = false;
      });
    }
  }
}
