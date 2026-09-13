import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  MessageDto,
  VectorSearchHitDto,
  type ChatCitationDto,
  type Contract,
  type ConversationDetailDto,
  type ConversationDto,
  type ConversationId,
  type MessageId,
  type ProviderDto,
  type RunHandleDto,
  type VectorStoreDto,
} from "@zvs/shared";
import type { EventRouter, RoutedEvent } from "../../app/EventRouter";
import ConversationVm from "./ConversationVm";

export default class ChatStore {
  conversations: ConversationDto[] = [];
  active: ConversationDetailDto | null = null;
  providers: ProviderDto[] = [];
  stores: VectorStoreDto[] = [];
  composer = new ConversationVm();
  query = "";
  contextOpen = false;
  loading = false;
  sending = false;
  stopping = false;
  error: string | null = null;
  run: RunHandleDto | null = null;
  liveText = "";
  liveReasoning = "";
  liveCitations: ChatCitationDto[] = [];
  searchedStoreIds: VectorStoreDto["id"][] = [];
  approvals: { id: string; subject: string; busy: boolean }[] = [];
  pendingTurn = false;
  outcome: "ok" | "cancelled" | "failed" | null = null;
  private buffer = "";
  private timer: ReturnType<typeof setTimeout> | undefined;
  private unsubscribe: (() => void) | undefined;
  private lastSeq = -1;
  private revision = 0;
  private mounted = false;
  private retryText = "";

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly events: EventRouter,
  ) {
    makeAutoObservable<
      ChatStore,
      | "ipc"
      | "events"
      | "buffer"
      | "timer"
      | "unsubscribe"
      | "lastSeq"
      | "revision"
      | "mounted"
      | "retryText"
    >(
      this,
      {
        ipc: false,
        events: false,
        buffer: false,
        timer: false,
        unsubscribe: false,
        lastSeq: false,
        revision: false,
        mounted: false,
        retryText: false,
      },
      { autoBind: true },
    );
  }

  get generating() {
    return this.sending || this.run !== null;
  }
  get canConfigure() {
    return !this.generating;
  }
  get provider() {
    return this.providers.find((provider) => provider.id === this.composer.providerId);
  }
  get modelId() {
    return this.composer.modelId;
  }
  get model() {
    return this.provider?.models.find(
      (model) => model.externalId === this.modelId || model.id === this.modelId,
    );
  }
  get attachedStoreIds() {
    return this.active?.attachedStoreIds ?? this.composer.attachedStoreIds;
  }
  get available() {
    return this.provider?.enabled === true && this.model?.available === true;
  }
  get canSend() {
    return (
      this.available && !this.loading && !this.generating && this.composer.text.trim().length > 0
    );
  }
  get canRetry() {
    return !this.generating && this.retryText.length > 0;
  }
  get sessionTokens() {
    return (
      this.active?.messages
        .filter((message) => message.role === "assistant")
        .reduce((sum, message) => sum + message.tokensIn + message.tokensOut, 0) ?? 0
    );
  }
  get visibleConversations() {
    return this.conversations
      .filter((conversation) =>
        conversation.title.toLowerCase().includes(this.query.trim().toLowerCase()),
      )
      .slice()
      .sort((a, b) => b.updatedAt - a.updatedAt);
  }
  setQuery(value: string) {
    this.query = value;
  }
  toggleContext() {
    this.contextOpen = !this.contextOpen;
  }

  async mount() {
    this.mounted = true;
    this.loading = true;
    this.error = null;
    const revision = ++this.revision;
    try {
      const [conversations, summaries, stores] = await Promise.all([
        this.ipc.call("chat.conversations.list", undefined),
        this.ipc.call("providers.list", { capability: "text", enabled: true }),
        this.ipc.call("vectorStores.list", undefined),
      ]);
      const providers = await Promise.all(
        summaries.map((provider) =>
          this.ipc.call("providers.get", { id: provider.id, selectedOnly: true }),
        ),
      );
      if (!this.mounted || revision !== this.revision) return;
      runInAction(() => {
        this.conversations = conversations;
        this.providers = providers;
        this.stores = stores;
        if (!this.composer.providerId && providers[0]) this.composer.selectProvider(providers[0]);
        this.loading = false;
      });
      if (this.run) this.subscribe();
      else if (this.active) await this.select(this.active.id);
    } catch (error) {
      runInAction(() => {
        if (revision === this.revision) {
          this.error = errorCopy(error);
          this.loading = false;
        }
      });
    }
  }

  async select(id: ConversationId) {
    if (this.generating) return;
    const revision = ++this.revision;
    this.loading = true;
    this.error = null;
    try {
      const active = await this.ipc.call("chat.conversations.get", { id });
      runInAction(() => {
        if (revision !== this.revision) return;
        this.active = active;
        const activeProvider = this.providers.find((provider) => provider.id === active.providerId);
        if (activeProvider) {
          this.composer.selectProvider(activeProvider);
          this.composer.setModel(active.modelId);
          this.composer.setTemperature(active.settings.temperature ?? this.composer.temperature);
        }
        this.liveText = "";
        this.liveReasoning = "";
        this.liveCitations = [];
        this.outcome = null;
        this.retryText = "";
        this.pendingTurn = false;
        this.composer.setText("");
      });
    } catch (error) {
      runInAction(() => {
        if (revision === this.revision) this.error = errorCopy(error);
      });
    } finally {
      runInAction(() => {
        if (revision === this.revision) this.loading = false;
      });
    }
  }

  newChat() {
    if (this.generating) return;
    ++this.revision;
    this.active = null;
    this.loading = false;
    this.error = null;
    this.outcome = null;
    this.liveText = "";
    this.liveReasoning = "";
    this.liveCitations = [];
    this.retryText = "";
    this.pendingTurn = false;
    this.composer.setText("");
  }

  async renameConversation(id: ConversationId) {
    if (this.generating) return;
    const conversation = this.conversations.find((item) => item.id === id);
    if (!conversation) return;
    return conversation;
  }

  async saveConversationTitle(id: ConversationId, title: string) {
    if (this.generating) return;
    const conversation = this.conversations.find((item) => item.id === id);
    const nextTitle = title.trim();
    if (!conversation || !nextTitle || nextTitle === conversation.title) return;
    const updated = await this.ipc.call("chat.conversations.rename", { id, title: nextTitle });
    runInAction(() => {
      this.conversations = this.conversations.map((item) => (item.id === id ? updated : item));
      if (this.active?.id === id) this.active = { ...this.active, ...updated };
    });
  }

  async removeConversation(id: ConversationId) {
    if (this.generating) return;
    const conversation = this.conversations.find((item) => item.id === id);
    if (!conversation) return;
    await this.ipc.call("chat.conversations.remove", { id });
    runInAction(() => {
      this.conversations = this.conversations.filter((item) => item.id !== id);
      if (this.active?.id === id) this.newChat();
    });
  }

  async send() {
    if (!this.canSend) return;
    const text = this.composer.text.trim();
    this.sending = true;
    this.error = null;
    this.outcome = null;
    this.retryText = text;
    this.liveText = "";
    this.liveCitations = [];
    this.searchedStoreIds = [];
    this.pendingTurn = true;
    try {
      if (!this.active) {
        const conversation = await this.ipc.call("chat.conversations.create", {
          title: "New conversation",
          providerId: this.provider!.id,
          modelId: this.modelId,
          settings: { temperature: this.composer.temperature },
          systemPrompt: this.composer.systemPrompt,
          attachedStoreIds: [...this.composer.attachedStoreIds],
        });
        runInAction(() => {
          this.active = { ...conversation, messages: [] };
          this.conversations.unshift(conversation);
        });
      }
      const id = this.active!.id;
      const run = await this.ipc.call("chat.send", {
        conversationId: id,
        text,
        providerId: this.composer.providerId ?? undefined,
        modelId: this.composer.modelId,
        settings: { temperature: this.composer.temperature },
      });
      runInAction(() => {
        this.run = run;
        this.lastSeq = -1;
        this.composer.setText("");
        this.sending = false;
        if (this.mounted) this.subscribe();
      });
    } catch (error) {
      runInAction(() => {
        this.error = errorCopy(error);
        this.outcome = "failed";
        this.sending = false;
        this.pendingTurn = false;
      });
    }
  }

  async truncateMessage(messageId: MessageId): Promise<boolean> {
    if (!this.active || this.generating) return false;
    try {
      const active = await this.ipc.call("chat.conversations.truncate", {
        id: this.active.id,
        messageId,
      });
      runInAction(() => {
        this.active = active;
        this.conversations = this.conversations.map((conversation) =>
          conversation.id === active.id ? active : conversation,
        );
        this.liveText = "";
        this.liveReasoning = "";
        this.liveCitations = [];
        this.pendingTurn = false;
        this.retryText = "";
      });
      return true;
    } catch (error) {
      this.error = errorCopy(error);
      return false;
    }
  }

  async deleteMessage(messageId: MessageId) {
    await this.truncateMessage(messageId);
  }

  async refreshMessage(messageId: MessageId, content: string) {
    if (!(await this.truncateMessage(messageId))) return;
    this.composer.setText(content);
    await this.send();
  }

  async editMessage(messageId: MessageId, content: string): Promise<boolean> {
    const text = content.trim();
    if (!text || !(await this.truncateMessage(messageId))) return false;
    this.composer.setText(text);
    await this.send();
    return true;
  }

  retry() {
    if (this.canRetry) {
      this.composer.setText(this.retryText);
      void this.send();
    }
  }
  get pendingUserText() {
    return this.pendingTurn ? this.retryText : "";
  }

  async decideApproval(id: string, approve: boolean) {
    const request = this.approvals.find((item) => item.id === id);
    if (!request || request.busy || !this.run) return;
    request.busy = true;
    try {
      if (approve)
        await this.ipc.call("runs.approve", { id: this.run.id, approvalId: id, always: false });
      else await this.ipc.call("runs.deny", { id: this.run.id, approvalId: id });
      runInAction(() => {
        this.approvals = this.approvals.filter((item) => item.id !== id);
      });
    } catch (error) {
      runInAction(() => {
        request.busy = false;
        this.error = errorCopy(error);
      });
    }
  }

  private subscribe() {
    this.unsubscribe?.();
    const run = this.run;
    if (!run) return;
    const stop = this.events.subscribe(run.streamId, this.receive);
    if (this.run === null) stop();
    else this.unsubscribe = stop;
  }

  private receive(event: RoutedEvent) {
    if (!this.run || event.streamId !== this.run.streamId || event.seq <= this.lastSeq) return;
    this.lastSeq = event.seq;
    if (
      event.type === "approval" &&
      event.request.runId === this.run.id &&
      typeof event.request.id === "string" &&
      typeof event.request.subject === "string"
    ) {
      if (!this.approvals.some((request) => request.id === event.request.id))
        this.approvals.push({ id: event.request.id, subject: event.request.subject, busy: false });
    }
    if (event.type === "token") {
      if (event.kind === "reasoning") this.liveReasoning += event.delta;
      else {
        this.buffer += event.delta;
        this.timer ??= setTimeout(this.flush, 32);
      }
    }
    if (event.type === "step" && event.step.status === "succeeded") {
      const nodeId = event.step.nodeId;
      if (typeof nodeId === "string" && /^store\d+$/.test(nodeId)) {
        const storeId = this.active?.attachedStoreIds[Number(nodeId.slice(5))];
        const hits = VectorSearchHitDto.array().safeParse(event.step.output);
        if (storeId && hits.success && !this.searchedStoreIds.includes(storeId))
          this.searchedStoreIds.push(storeId);
        if (storeId && hits.success)
          this.liveCitations = [
            ...this.liveCitations.filter((citation) => citation.storeId !== storeId),
            ...hits.data.map((hit) => ({
              storeId,
              documentId: hit.documentId,
              sourcePath: hit.path,
              chunkIndex: hit.chunkIndex,
              score: hit.score,
            })),
          ];
      }
      if (nodeId === "persist") {
        const message = MessageDto.safeParse(event.step.output);
        if (message.success) {
          this.flush();
          this.liveText = message.data.content;
          this.liveReasoning = message.data.reasoning;
          this.liveCitations = message.data.citations;
        }
      }
    }
    if (event.type === "end") {
      this.flush();
      this.outcome = event.outcome.status;
      if (event.outcome.status === "failed")
        this.error = event.outcome.message ?? "Generation failed. Try again.";
      this.unsubscribe?.();
      this.unsubscribe = undefined;
      this.run = null;
      this.approvals = [];
      this.stopping = false;
      void this.refreshCompleted();
    }
  }

  private flush() {
    if (this.timer !== undefined) clearTimeout(this.timer);
    this.timer = undefined;
    if (this.buffer) {
      this.liveText += this.buffer;
      this.buffer = "";
    }
  }

  private async refreshCompleted() {
    const id = this.active?.id;
    const revision = this.revision;
    if (!id) return;
    this.loading = true;
    try {
      const active = await this.ipc.call("chat.conversations.get", { id });
      runInAction(() => {
        if (revision !== this.revision) return;
        this.active = active;
        this.conversations = this.conversations.map((conversation) =>
          conversation.id === id ? active : conversation,
        );
        this.liveText = "";
        this.liveReasoning = "";
        this.liveCitations = [];
        this.pendingTurn = false;
      });
    } catch (error) {
      runInAction(() => {
        if (revision === this.revision) this.error = errorCopy(error);
      });
    } finally {
      runInAction(() => {
        if (revision === this.revision) this.loading = false;
      });
    }
  }

  async stop() {
    if (!this.run || this.stopping) return;
    this.stopping = true;
    try {
      await this.ipc.call("chat.cancel", { id: this.run.id });
    } catch (error) {
      runInAction(() => {
        this.error = errorCopy(error);
        this.stopping = false;
      });
    }
  }

  dispose() {
    this.mounted = false;
    ++this.revision;
    this.loading = false;
    this.flush();
    this.unsubscribe?.();
    this.unsubscribe = undefined;
  }
}

function errorCopy(error: unknown) {
  return error instanceof Error ? error.message : "Unable to load chat. Please try again.";
}
