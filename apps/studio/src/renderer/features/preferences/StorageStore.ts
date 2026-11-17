import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract, StorageCategory, StorageUsageDto } from "@zvs/shared";
import {
  STORAGE_RESET_FAILED,
  STORAGE_UNAVAILABLE,
  clearErrorCopy,
  clearedCopy,
} from "./storageCopy";

export default class StorageStore {
  usage: StorageUsageDto | null = null;
  loading = false;
  clearing = false;
  error: string | null = null;
  notice: string | null = null;
  resetChallenge: string | null = null;
  resetInput = "";
  resetting = false;
  resetLoading = false;
  resetError: string | null = null;
  expanded = new Set<StorageCategory>();

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<StorageStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  get categories() {
    return [...(this.usage?.categories ?? [])]
      .sort((a, b) => b.bytes - a.bytes)
      .map((item) => ({
        ...item,
        parts: [...(item.parts ?? [])].sort((a, b) => b.bytes - a.bytes),
      }));
  }

  isExpanded(category: StorageCategory) {
    return this.expanded.has(category);
  }

  toggle(category: StorageCategory) {
    if (this.expanded.has(category)) this.expanded.delete(category);
    else this.expanded.add(category);
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

  get canReset() {
    return (
      this.resetChallenge !== null && this.resetInput === this.resetChallenge && !this.resetting
    );
  }

  setResetInput(value: string) {
    this.resetInput = value;
  }

  closeReset() {
    if (this.resetting) return;
    this.resetChallenge = null;
    this.resetInput = "";
  }

  async openReset() {
    if (this.resetLoading || this.resetting) return;
    this.resetLoading = true;
    this.resetError = null;
    this.resetChallenge = null;
    this.resetInput = "";
    try {
      const { challenge } = await this.ipc.call("storage.resetChallenge", undefined);
      runInAction(() => {
        this.resetChallenge = challenge;
      });
    } catch {
      runInAction(() => {
        this.resetError = STORAGE_RESET_FAILED;
      });
    } finally {
      runInAction(() => {
        this.resetLoading = false;
      });
    }
  }

  async reset() {
    if (!this.canReset) return;
    this.resetting = true;
    this.resetError = null;
    try {
      await this.ipc.call("storage.reset", { challenge: this.resetInput });
    } catch {
      runInAction(() => {
        this.resetting = false;
        this.resetError = STORAGE_RESET_FAILED;
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
