import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract, StorageUsageDto } from "@zvs/shared";
import { STORAGE_UNAVAILABLE, clearErrorCopy, clearedCopy } from "./storageCopy";

export default class StorageStore {
  usage: StorageUsageDto | null = null;
  loading = false;
  clearing = false;
  error: string | null = null;
  notice: string | null = null;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<StorageStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  get categories() {
    return [...(this.usage?.categories ?? [])].sort((a, b) => b.bytes - a.bytes);
  }

  async load(refresh = false) {
    if (this.loading) return;
    this.loading = true;
    this.error = null;
    try {
      const usage = await this.ipc.call("storage.usage", { refresh });
      runInAction(() => {
        this.usage = usage;
      });
    } catch {
      runInAction(() => {
        this.error = STORAGE_UNAVAILABLE;
      });
    } finally {
      runInAction(() => {
        this.loading = false;
      });
    }
  }

  async clearCache() {
    if (this.clearing) return;
    this.clearing = true;
    this.error = null;
    this.notice = null;
    try {
      const { removedBytes } = await this.ipc.call("storage.clearCache", undefined);
      runInAction(() => {
        this.notice = clearedCopy(removedBytes);
      });
      await this.load(true);
    } catch (error) {
      runInAction(() => {
        this.error = clearErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.clearing = false;
      });
    }
  }
}
