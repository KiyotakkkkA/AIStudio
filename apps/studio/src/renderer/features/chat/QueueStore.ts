import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  CHAT_QUEUE_MAX,
  ChatQueueStepDto,
  type ChatQueueDto,
  type ChatQueuedMessageDto,
  type Contract,
  type ConversationId,
} from "@zvs/shared";
import {
  QUEUE_ADD_FAILED,
  QUEUE_EDIT_FAILED,
  QUEUE_REMOVE_FAILED,
  QUEUE_SEND_FAILED,
} from "./queueCopy";

export interface QueueSource {
  readonly conversationId: () => ConversationId | null;
  readonly sent: (id: ConversationId) => void;
}

export default class QueueStore {
  queue: ChatQueueDto | null = null;
  selectedId: string | null = null;
  editingId: string | null = null;
  draft = "";
  busy = false;
  error: string | null = null;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly source: QueueSource,
  ) {
    makeAutoObservable<QueueStore, "ipc" | "source">(
      this,
      { ipc: false, source: false },
      { autoBind: true },
    );
  }

  get messages(): readonly ChatQueuedMessageDto[] {
    const queue = this.queue;
    if (queue === null || queue.conversationId !== this.source.conversationId()) return [];
    return queue.messages;
  }

  get urgent(): boolean {
    return this.messages.length > 0 && this.queue?.urgent === true;
  }

  get full(): boolean {
    return this.messages.length >= CHAT_QUEUE_MAX;
  }

  get selected(): ChatQueuedMessageDto | null {
    return this.messages.find((message) => message.id === this.selectedId) ?? null;
  }

  get editing(): ChatQueuedMessageDto | null {
    return this.messages.find((message) => message.id === this.editingId) ?? null;
  }

  reset() {
    this.queue = null;
    this.selectedId = null;
    this.editingId = null;
    this.draft = "";
    this.error = null;
  }

  async load(id: ConversationId) {
    try {
      const queue = await this.ipc.call("chat.queue.list", { conversationId: id });
      runInAction(() => {
        if (this.source.conversationId() === id) this.accept(queue);
      });
    } catch {
      runInAction(() => {
        if (this.source.conversationId() === id) this.queue = null;
      });
    }
  }

  apply(step: Record<string, unknown>): boolean {
    const parsed = ChatQueueStepDto.safeParse(step);
    if (!parsed.success) return false;
    if (parsed.data.conversationId !== this.source.conversationId()) return true;
    this.accept({
      conversationId: parsed.data.conversationId,
      messages: parsed.data.messages,
      urgent: parsed.data.urgent,
    });
    return true;
  }

  select(id: string | null) {
    this.selectedId = id;
  }

  move(delta: number) {
    const messages = this.messages;
    if (messages.length === 0) return;
    const index = messages.findIndex((message) => message.id === this.selectedId);
    const next =
      index < 0
        ? delta > 0
          ? 0
          : messages.length - 1
        : Math.min(messages.length - 1, Math.max(0, index + delta));
    this.selectedId = messages[next]?.id ?? null;
  }

  startEdit(id?: string) {
    const target = this.messages.find((message) => message.id === (id ?? this.selectedId));
    if (target === undefined) return;
    this.selectedId = target.id;
    this.editingId = target.id;
    this.draft = target.text;
    this.error = null;
  }

  setDraft(text: string) {
    this.draft = text;
  }

  cancelEdit() {
    this.editingId = null;
    this.draft = "";
  }

  async add(text: string): Promise<boolean> {
    const id = this.source.conversationId();
    const trimmed = text.trim();
    if (id === null || trimmed.length === 0 || this.full) return false;
    return this.call(QUEUE_ADD_FAILED, () =>
      this.ipc.call("chat.queue.add", { conversationId: id, text: trimmed }),
    );
  }

  async saveEdit(): Promise<boolean> {
    const id = this.source.conversationId();
    const messageId = this.editingId;
    const text = this.draft.trim();
    if (id === null || messageId === null || text.length === 0) return false;
    const saved = await this.call(QUEUE_EDIT_FAILED, () =>
      this.ipc.call("chat.queue.edit", { conversationId: id, messageId, text }),
    );
    if (saved) this.cancelEdit();
    return saved;
  }

  async remove(messageId?: string): Promise<boolean> {
    const id = this.source.conversationId();
    const target = messageId ?? this.selectedId;
    if (id === null || target === null) return false;
    const removed = await this.call(QUEUE_REMOVE_FAILED, () =>
      this.ipc.call("chat.queue.remove", { conversationId: id, messageId: target }),
    );
    if (removed && this.editingId === target) this.cancelEdit();
    return removed;
  }

  async sendNow(): Promise<boolean> {
    const id = this.source.conversationId();
    if (id === null || this.messages.length === 0) return false;
    return this.call(QUEUE_SEND_FAILED, () =>
      this.ipc.call("chat.queue.flush", { conversationId: id }),
    );
  }

  private async call(failure: string, request: () => Promise<ChatQueueDto>): Promise<boolean> {
    if (this.busy) return false;
    this.busy = true;
    this.error = null;
    try {
      const queue = await request();
      runInAction(() => {
        if (queue.conversationId === this.source.conversationId()) this.accept(queue);
      });
      return true;
    } catch {
      runInAction(() => {
        this.error = failure;
      });
      return false;
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }

  private accept(queue: ChatQueueDto) {
    const before = this.messages.length;
    this.queue = queue;
    const ids = new Set(queue.messages.map((message) => message.id));
    if (this.selectedId !== null && !ids.has(this.selectedId))
      this.selectedId = queue.messages.at(-1)?.id ?? null;
    if (this.editingId !== null && !ids.has(this.editingId)) this.cancelEdit();
    if (before > 0 && queue.messages.length === 0) this.source.sent(queue.conversationId);
  }
}
