import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type {
  ConversationDto,
  ConversationId,
  Contract,
  MemoryChatDto,
  MemoryDto,
  MemoryId,
} from "@zvs/shared";
import { LOAD_ERROR, memoryErrorCopy } from "./memoryCopy";

export interface DialogMemory {
  readonly conversation: MemoryChatDto;
  readonly memories: readonly MemoryDto[];
}

export default class MemoryStore {
  memories: MemoryDto[] = [];
  conversations: ConversationDto[] = [];
  loading = false;
  loaded = false;
  saving = false;
  error: string | null = null;
  private revision = 0;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<MemoryStore, "ipc" | "revision">(
      this,
      { ipc: false, revision: false },
      { autoBind: true },
    );
  }

  get account(): MemoryDto[] {
    return this.memories.filter((memory) => memory.scope === "account");
  }

  get dialogs(): DialogMemory[] {
    const groups = new Map<ConversationId, DialogMemory>();
    for (const memory of this.memories) {
      if (memory.conversation === null) continue;
      const group = groups.get(memory.conversation.id);
      groups.set(memory.conversation.id, {
        conversation: memory.conversation,
        memories: [...(group?.memories ?? []), memory],
      });
    }
    return [...groups.values()];
  }

  dismissError() {
    this.error = null;
  }

  async load() {
    const revision = ++this.revision;
    this.loading = true;
    try {
      const [memories, conversations] = await Promise.all([
        this.ipc.call("memory.list", undefined),
        this.ipc.call("chat.conversations.list", undefined),
      ]);
      runInAction(() => {
        if (revision !== this.revision) return;
        this.memories = memories;
        this.conversations = conversations;
        this.loaded = true;
        if (this.error === LOAD_ERROR) this.error = null;
      });
    } catch {
      runInAction(() => {
        if (revision === this.revision) this.error = LOAD_ERROR;
      });
    } finally {
      runInAction(() => {
        if (revision === this.revision) this.loading = false;
      });
    }
  }

  async add(content: string, conversationId?: ConversationId): Promise<boolean> {
    return this.mutate(() =>
      this.ipc.call(
        "memory.create",
        conversationId === undefined
          ? { scope: "account", content }
          : { scope: "conversation", conversationId, content },
      ),
    );
  }

  async edit(id: MemoryId, content: string): Promise<boolean> {
    return this.mutate(() => this.ipc.call("memory.update", { id, content }));
  }

  async remove(id: MemoryId): Promise<boolean> {
    return this.mutate(() => this.ipc.call("memory.remove", { id }));
  }

  async clear(conversationId?: ConversationId): Promise<boolean> {
    return this.mutate(() =>
      this.ipc.call(
        "memory.clear",
        conversationId === undefined
          ? { scope: "account" }
          : { scope: "conversation", conversationId },
      ),
    );
  }

  private async mutate(work: () => Promise<unknown>): Promise<boolean> {
    if (this.saving) return false;
    this.saving = true;
    this.error = null;
    let done = false;
    try {
      await work();
      done = true;
    } catch (error) {
      runInAction(() => {
        this.error = memoryErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.saving = false;
      });
    }
    await this.load();
    return done;
  }

  dispose() {
    ++this.revision;
  }
}
