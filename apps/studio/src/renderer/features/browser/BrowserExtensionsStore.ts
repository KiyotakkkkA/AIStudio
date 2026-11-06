import { makeAutoObservable, runInAction } from "mobx";
import { isIpcError, type IpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  type BrowserExtensionDto,
  type BrowserExtensionsDto,
  type Contract,
} from "@zvs/shared";
import {
  EXTENSION_ACTION_FAILED,
  EXTENSION_INVALID,
  EXTENSIONS_FULL,
  EXTENSIONS_LOAD_FAILED,
} from "./browserExtensionsCopy";

function failure(error: unknown): string {
  if (isIpcError(error) && error.code === AppErrorCode.VALIDATION_FAILED) return EXTENSION_INVALID;
  if (isIpcError(error) && error.code === AppErrorCode.CONFLICT) return EXTENSIONS_FULL;
  return EXTENSION_ACTION_FAILED;
}

export default class BrowserExtensionsStore {
  items: BrowserExtensionDto[] = [];
  loaded = false;
  busy = false;
  error: string | null = null;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<BrowserExtensionsStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  async load() {
    try {
      const list = await this.ipc.call("browserExtensions.list", undefined);
      runInAction(() => {
        this.items = list.items;
        this.loaded = true;
      });
    } catch {
      runInAction(() => {
        this.loaded = true;
        this.error = EXTENSIONS_LOAD_FAILED;
      });
    }
  }

  private async change(work: () => Promise<BrowserExtensionsDto>) {
    if (this.busy) return;
    this.busy = true;
    this.error = null;
    try {
      const list = await work();
      runInAction(() => {
        this.items = list.items;
      });
    } catch (error) {
      runInAction(() => {
        this.error = failure(error);
      });
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }

  add() {
    return this.change(() => this.ipc.call("browserExtensions.add", undefined));
  }

  remove(id: string) {
    return this.change(() => this.ipc.call("browserExtensions.remove", { id }));
  }

  setEnabled(id: string, enabled: boolean) {
    return this.change(() => this.ipc.call("browserExtensions.setEnabled", { id, enabled }));
  }

  dismissError() {
    this.error = null;
  }
}
