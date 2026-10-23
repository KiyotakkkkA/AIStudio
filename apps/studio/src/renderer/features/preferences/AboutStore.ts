import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract, DeviceProfileDto } from "@zvs/shared";
import { DEVICE_UNAVAILABLE } from "./preferencesPresentation";

export default class AboutStore {
  device: DeviceProfileDto | null = null;
  loading = false;
  error: string | null = null;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<AboutStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  async load() {
    if (this.loading || this.device !== null) return;
    this.loading = true;
    this.error = null;
    try {
      const device = await this.ipc.call("system.device", { refresh: false });
      runInAction(() => {
        this.device = device;
      });
    } catch {
      runInAction(() => {
        this.error = DEVICE_UNAVAILABLE;
      });
    } finally {
      runInAction(() => {
        this.loading = false;
      });
    }
  }
}
