import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { BrowserDownloadDto, BrowserDownloadsDto, Contract } from "@zvs/shared";
import { DOWNLOAD_ACTION_FAILED, DOWNLOADS_LOAD_FAILED } from "./browserDataCopy";

export default class BrowserDownloadsStore {
  items: BrowserDownloadDto[] = [];
  loaded = false;
  busy = false;
  error: string | null = null;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<BrowserDownloadsStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  async load() {
    try {
      const downloads = await this.ipc.call("browserDownloads.list", undefined);
      runInAction(() => {
        this.items = downloads.items;
        this.loaded = true;
      });
    } catch {
      runInAction(() => {
        this.loaded = true;
        this.error = DOWNLOADS_LOAD_FAILED;
      });
    }
  }

  private async change(work: () => Promise<BrowserDownloadsDto>) {
    this.busy = true;
    this.error = null;
    try {
      const downloads = await work();
      runInAction(() => {
        this.items = downloads.items;
      });
    } catch {
      runInAction(() => {
        this.error = DOWNLOAD_ACTION_FAILED;
      });
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }

  remove(id: string) {
    return this.change(() => this.ipc.call("browserDownloads.remove", { id }));
  }

  clear() {
    return this.change(() => this.ipc.call("browserDownloads.clear", undefined));
  }

  private async act(channel: "browserDownloads.open" | "browserDownloads.reveal", id: string) {
    this.error = null;
    try {
      await this.ipc.call(channel, { id });
    } catch {
      runInAction(() => {
        this.error = DOWNLOAD_ACTION_FAILED;
      });
    }
  }

  open(id: string) {
    return this.act("browserDownloads.open", id);
  }

  reveal(id: string) {
    return this.act("browserDownloads.reveal", id);
  }

  dismissError() {
    this.error = null;
  }
}
