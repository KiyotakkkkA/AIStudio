import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { ChatContextInput, ChatStatsDto, Contract } from "@zvs/shared";

export default class ChatStatsStore {
  open = false;
  stats: ChatStatsDto | null = null;
  loading = false;
  failed = false;
  private revision = 0;
  private input: ChatContextInput | null = null;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<ChatStatsStore, "ipc" | "revision" | "input">(
      this,
      { ipc: false, revision: false, input: false },
      { autoBind: true },
    );
  }

  toggle(): void {
    this.open = !this.open;
    if (this.open) void this.load();
  }

  close(): void {
    this.open = false;
  }

  async refresh(input: ChatContextInput | null): Promise<void> {
    this.input = input;
    if (this.open) await this.load();
  }

  async load(): Promise<void> {
    const input = this.input;
    const revision = ++this.revision;
    if (input === null) {
      this.stats = null;
      this.loading = false;
      this.failed = false;
      return;
    }
    this.loading = true;
    this.failed = false;
    try {
      const stats = await this.ipc.call("chat.stats", input);
      runInAction(() => {
        if (revision !== this.revision) return;
        this.stats = stats;
        this.loading = false;
      });
    } catch {
      runInAction(() => {
        if (revision !== this.revision) return;
        this.failed = true;
        this.loading = false;
      });
    }
  }
}
