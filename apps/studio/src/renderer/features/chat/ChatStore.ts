import { makeAutoObservable, reaction, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  ChatToolCallDto,
  FilesApprovalDto,
  FolderApprovalDto,
  ProcessApprovalDto,
  type FileDiffDto,
  type ProcessPlanDto,
  MessageDto,
  VectorSearchHitDto,
  modelToolSupport,
  resolveToolMode,
  isAccountFamily,
  type ToolMode,
  type AccountDto,
  type AccountFamily,
  type AccountId,
  type AdapterDescriptorDto,
  type ApprovalRequestDto,
  type ChatAttachDto,
  type ChatCitationDto,
  type ChatContextDto,
  type ChatContextInput,
  type Contract,
  type ConversationDetailDto,
  type ConversationDto,
  ConversationId,
  type DocumentSelectionDto,
  type FolderAccessAnswer,
  type FolderGrantScope,
  type McpServerDto,
  type MessageId,
  type ProviderDto,
  type QuestionAnswerDto,
  type QuestionRequestDto,
  type RetrievalMode,
  type RunHandleDto,
  type RunOutcomeDto,
  type ToolDto,
  type VectorStoreDto,
  type VendorTurnOptions,
  type WebSourceDto,
} from "@zvs/shared";
import type { EventRouter, RoutedEvent } from "../../app/EventRouter";
import ConversationVm from "./ConversationVm";
import { adapterLabel } from "../providers/accountPresentation";
import { suggestedBaseUrl } from "../providers/providerPresentation";
import { answerErrorCopy, folderErrorCopy } from "./workingFolderCopy";
import ArtifactActions from "./ArtifactActions";
import ExplorerStore from "./explorer/ExplorerStore";
import TerminalStore from "../terminal/TerminalStore";
import {
  automaticOffers,
  CORE_TOOLS,
  isCoreTool,
  isModelTool,
  policyTools,
  RECOMMENDED_TOOLS,
  toolBlockReason,
} from "./toolSets";

export const ACCOUNT_OPTION_PREFIX = "account:";
const LAST_CONVERSATION_KEY = "chat.conversation.last";
export const SELECTIONS_FULL = "К сообщению можно приложить не больше 8 фрагментов документов.";
export const GENERATION_FAILED = "Не удалось получить ответ. Повторите запрос.";

export type SidePanel = "context" | "files";

export interface AccountChoice {
  readonly value: string;
  readonly label: string;
  readonly accountId: AccountId;
  readonly adapter: AccountFamily;
  readonly detail: string | null;
}

export default class ChatStore {
  conversations: ConversationDto[] = [];
  active: ConversationDetailDto | null = null;
  providers: ProviderDto[] = [];
  stores: VectorStoreDto[] = [];
  tools: ToolDto[] = [];
  accounts: AccountDto[] = [];
  adapters: AdapterDescriptorDto[] = [];
  servers: McpServerDto[] = [];
  context: ChatContextDto | null = null;
  connecting = false;
  connectingAccountId: AccountId | null = null;
  composer = new ConversationVm();
  query = "";
  sidePanel: SidePanel = "context";
  panelOpen = true;
  listOpen = true;
  loading = false;
  sending = false;
  stopping = false;
  error: string | null = null;
  run: RunHandleDto | null = null;
  runStartedAt: number | null = null;
  liveText = "";
  liveReasoning = "";
  liveCitations: ChatCitationDto[] = [];
  liveWebSources: WebSourceDto[] = [];
  liveSelections: DocumentSelectionDto[] = [];
  liveToolCalls: ChatToolCallDto[] = [];
  searchedStoreIds: VectorStoreDto["id"][] = [];
  approvals: {
    id: string;
    subject: string;
    busy: boolean;
    changes: FileDiffDto[];
    process?: ProcessPlanDto;
  }[] = [];
  folderRequests: FolderApprovalDto[] = [];
  questions: { request: QuestionRequestDto; busy: boolean }[] = [];
  answeringFolder = false;
  folderBusy = false;
  folderError: string | null = null;
  folderPrompt: { path: string } | null = null;
  pendingTurn = false;
  readonly explorer: ExplorerStore;
  readonly terminal: TerminalStore;
  readonly artifacts: ArtifactActions;
  outcome: "ok" | "cancelled" | "failed" | null = null;
  revealed = 0;
  private buffer = "";
  private timer: ReturnType<typeof setTimeout> | undefined;
  private unsubscribe: (() => void) | undefined;
  private lastSeq = -1;
  private revision = 0;
  private mounted = false;
  private pendingOpen: ConversationId | null = null;
  private remembered: ConversationId | null | undefined;
  private ready = false;
  private retryText = "";
  private contextRevision = 0;
  private stopContext: (() => void) | undefined;
  private attaching = false;
  private contiguous = false;
  private held: RoutedEvent[] = [];

  private memoryTool: () => boolean = () => false;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly events: EventRouter,
  ) {
    this.explorer = new ExplorerStore(ipc, events);
    this.terminal = new TerminalStore(ipc, events);
    this.artifacts = new ArtifactActions(ipc);
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
      | "pendingOpen"
      | "remembered"
      | "ready"
      | "retryText"
      | "contextRevision"
      | "stopContext"
      | "attaching"
      | "contiguous"
      | "held"
      | "memoryTool"
    >(
      this,
      {
        ipc: false,
        events: false,
        explorer: false,
        terminal: false,
        artifacts: false,
        buffer: false,
        timer: false,
        unsubscribe: false,
        memoryTool: false,
        lastSeq: false,
        revision: false,
        mounted: false,
        pendingOpen: false,
        remembered: false,
        ready: false,
        retryText: false,
        contextRevision: false,
        stopContext: false,
        attaching: false,
        contiguous: false,
        held: false,
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
  get accountFamily(): AccountFamily | null {
    const provider = this.provider;
    if (provider?.authMode !== "account" || !isAccountFamily(provider.adapter)) return null;
    return provider.adapter;
  }
  get vendorOptions(): VendorTurnOptions | undefined {
    if (this.accountFamily !== "qwen-web") return undefined;
    return { effort: this.composer.effort, webSearch: this.composer.webSearch };
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
  get useRelations() {
    return this.active?.useRelations ?? this.composer.useRelations;
  }
  get workingFolder() {
    return this.active?.workingFolder ?? null;
  }
  get explorerOwner() {
    return this.active?.id ?? null;
  }
  get workingFolderAccess() {
    return this.active?.workingFolderAccess ?? null;
  }
  get canChangeFolder() {
    return !this.generating && !this.folderBusy && !this.loading;
  }
  get folderRequest() {
    return this.folderRequests[0] ?? null;
  }
  get retrievalMode(): RetrievalMode {
    return this.active?.retrievalMode ?? this.composer.retrievalMode;
  }
  get toolMode(): ToolMode {
    const provider = this.provider;
    const model = this.model;
    if (!provider || !model) return "none";
    return resolveToolMode(
      provider.adapterCapabilities.tools,
      modelToolSupport(model, provider.models),
    );
  }
  get accountChoices(): AccountChoice[] {
    return this.accounts
      .filter(
        (account) =>
          account.status === "linked" &&
          this.adapters.some(
            (adapter) =>
              adapter.family === account.adapter &&
              adapter.implemented &&
              adapter.streaming &&
              adapter.authModes.includes("account"),
          ) &&
          !this.providers.some(
            (provider) =>
              provider.accountId === account.id &&
              provider.enabled &&
              provider.models.some((model) => model.available),
          ),
      )
      .map((account) => ({
        value: `${ACCOUNT_OPTION_PREFIX}${account.id}`,
        label: `${adapterLabel(account.adapter)} · аккаунт`,
        accountId: account.id,
        adapter: account.adapter,
        detail: account.emailMasked ?? account.displayName,
      }));
  }
  async connectAccount(accountId: AccountId) {
    const account = this.accounts.find((entry) => entry.id === accountId);
    if (!account || this.connecting) return;
    this.connecting = true;
    this.connectingAccountId = account.id;
    this.error = null;
    try {
      const created =
        (await this.accountProvider(account)) ??
        (await this.ipc.call("providers.create", {
          name: adapterLabel(account.adapter),
          kind: "openai-compatible",
          adapter: account.adapter,
          authMode: "account",
          baseUrl: suggestedBaseUrl("openai-compatible", account.adapter),
          secretId: null,
          accountId: account.id,
          capabilities: ["text"],
          settings: { selectedModelIds: [] },
          enabled: true,
        }));
      await this.ipc.call("providers.probe", { id: created.id });
      const discovered = await this.ipc.call("providers.get", {
        id: created.id,
        selectedOnly: false,
      });
      const selectedModelIds = discovered.models
        .filter((model) => model.available)
        .map((model) => model.externalId);
      await this.ipc.call("providers.update", {
        id: created.id,
        enabled: true,
        capabilities: discovered.capabilities.includes("text")
          ? discovered.capabilities
          : [...discovered.capabilities, "text"],
        settings: { ...discovered.settings, selectedModelIds },
      });
      const provider = await this.ipc.call("providers.get", {
        id: created.id,
        selectedOnly: true,
      });
      runInAction(() => {
        this.providers = [...this.providers.filter((item) => item.id !== provider.id), provider];
        this.composer.selectProvider(provider);
        this.connecting = false;
        this.connectingAccountId = null;
      });
    } catch (error) {
      runInAction(() => {
        this.error = errorCopy(error);
        this.connecting = false;
        this.connectingAccountId = null;
      });
    }
  }
  private async accountProvider(account: AccountDto) {
    const summaries = await this.ipc.call("providers.list", {});
    for (const summary of summaries) {
      if (summary.adapter !== account.adapter || summary.authMode !== "account") continue;
      const provider = await this.ipc.call("providers.get", {
        id: summary.id,
        selectedOnly: false,
      });
      if (provider.accountId === account.id) return provider;
    }
    return null;
  }
  get enabledTools() {
    return this.active?.enabledTools ?? this.composer.enabledTools;
  }
  get automaticTools() {
    const offered = automaticOffers(this.active?.messages ?? [], this.enabledTools);
    if (this.toolMode === "none") return offered;
    return policyTools(offered, {
      attachedStores: this.attachedStoreIds.length,
      retrievalMode: this.retrievalMode,
      selected: this.enabledTools,
      memory: this.memoryTool(),
    });
  }
  useMemorySwitch(source: () => boolean) {
    this.memoryTool = source;
  }
  get coreTools(): readonly string[] {
    return this.toolMode === "none" ? [] : CORE_TOOLS;
  }
  get selectableTools() {
    return this.tools
      .filter(
        (tool) =>
          isModelTool(tool.key) &&
          !isCoreTool(tool.key) &&
          (tool.available || this.enabledTools.includes(tool.key)),
      )
      .slice()
      .sort((left, right) => left.key.localeCompare(right.key));
  }
  get offeredToolCount() {
    return this.tools.filter(
      (tool) =>
        this.enabledTools.includes(tool.key) &&
        tool.available &&
        tool.enabled &&
        tool.tier !== "off",
    ).length;
  }
  get standaloneApprovals() {
    const attached = new Set(
      this.liveToolCalls.flatMap((call) => (call.approvalId === null ? [] : [call.approvalId])),
    );
    return this.approvals.filter((request) => !attached.has(request.id));
  }
  approvalFor(id: string | null) {
    return id === null ? undefined : this.approvals.find((request) => request.id === id);
  }
  questionFor(id: string) {
    return this.questions.find((question) => question.request.id === id);
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
  get lastTurn() {
    return this.active?.messages
      .slice()
      .reverse()
      .find((message) => message.role === "assistant" && !message.partial);
  }
  get contextUsed() {
    return this.context?.total ?? 0;
  }
  get contextWindow() {
    return this.context?.contextWindow ?? this.model?.contextWindow ?? 0;
  }
  private get contextInput(): ChatContextInput | null {
    const providerId = this.composer.providerId;
    const modelId = this.model?.externalId;
    if (!providerId || !modelId || !this.available) return null;
    if (this.active)
      return {
        conversationId: this.active.id,
        providerId,
        modelId,
        enabledTools: [...this.active.enabledTools],
      };
    return {
      providerId,
      modelId,
      systemPrompt: this.composer.systemPrompt,
      enabledTools: [...this.composer.enabledTools],
    };
  }
  async refreshContext() {
    const input = this.contextInput;
    const revision = ++this.contextRevision;
    if (!input) {
      this.context = null;
      return;
    }
    try {
      const context = await this.ipc.call("chat.context", input);
      runInAction(() => {
        if (revision === this.contextRevision) this.context = context;
      });
    } catch {
      runInAction(() => {
        if (revision === this.contextRevision) this.context = null;
      });
    }
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
  toggleList() {
    this.listOpen = !this.listOpen;
  }
  get activePanel(): SidePanel {
    return this.sidePanel === "files" && this.workingFolder !== null ? "files" : "context";
  }
  isPanelShown(panel: SidePanel) {
    return this.panelOpen && this.activePanel === panel;
  }
  showPanel(panel: SidePanel) {
    if (this.activePanel === panel) {
      this.panelOpen = !this.panelOpen;
      return;
    }
    this.sidePanel = panel;
    this.panelOpen = true;
  }
  closePanel() {
    this.panelOpen = false;
  }
  attachSelection(selection: DocumentSelectionDto) {
    if (this.composer.addSelection(selection)) this.error = null;
    else this.error = SELECTIONS_FULL;
  }

  async toggleStore(id: VectorStoreDto["id"]) {
    if (!this.canConfigure) return;
    const current = this.attachedStoreIds;
    const attachedStoreIds = current.includes(id)
      ? current.filter((value) => value !== id)
      : [...current, id];
    if (!this.active) {
      this.composer.attachedStoreIds = attachedStoreIds;
      return;
    }
    try {
      const updated = await this.ipc.call("chat.conversations.updateStores", {
        id: this.active.id,
        attachedStoreIds,
      });
      runInAction(() => {
        this.active = { ...this.active!, ...updated };
        this.conversations = this.conversations.map((conversation) =>
          conversation.id === updated.id ? updated : conversation,
        );
      });
    } catch (error) {
      this.error = errorCopy(error);
    }
  }

  get recommendedTools() {
    return RECOMMENDED_TOOLS.filter((key) =>
      this.tools.some((tool) => tool.key === key && toolBlockReason(tool) === null),
    );
  }

  get usesRecommendedTools() {
    const current = this.enabledTools;
    return (
      current.length === this.recommendedTools.length &&
      this.recommendedTools.every((key) => current.includes(key))
    );
  }

  toggleTool(key: ToolDto["key"]) {
    if (isCoreTool(key)) return Promise.resolve();
    const current = this.enabledTools;
    return this.setTools(
      current.includes(key) ? current.filter((value) => value !== key) : [...current, key],
    );
  }

  useRecommendedTools() {
    return this.setTools(this.recommendedTools);
  }

  private async setTools(keys: readonly string[]) {
    if (!this.canConfigure) return;
    const enabledTools = keys.filter((key) => !isCoreTool(key));
    if (!this.active) {
      this.composer.setEnabledTools(enabledTools);
      return;
    }
    try {
      const updated = await this.ipc.call("chat.conversations.updateTools", {
        id: this.active.id,
        enabledTools,
      });
      runInAction(() => {
        this.active = { ...this.active!, ...updated };
        this.conversations = this.conversations.map((conversation) =>
          conversation.id === updated.id ? updated : conversation,
        );
      });
    } catch (error) {
      this.error = errorCopy(error);
    }
  }

  async refreshTools() {
    try {
      const tools = await this.ipc.call("tools.list", { includeUnavailable: true });
      runInAction(() => {
        this.tools = tools;
      });
    } catch (error) {
      this.error = errorCopy(error);
    }
  }

  async setRetrievalMode(retrievalMode: RetrievalMode) {
    if (!this.canConfigure || retrievalMode === this.retrievalMode) return;
    if (!this.active) {
      this.composer.setRetrievalMode(retrievalMode);
      return;
    }
    try {
      const updated = await this.ipc.call("chat.conversations.updateRetrieval", {
        id: this.active.id,
        retrievalMode,
      });
      runInAction(() => {
        this.active = { ...this.active!, ...updated };
        this.conversations = this.conversations.map((conversation) =>
          conversation.id === updated.id ? updated : conversation,
        );
      });
    } catch (error) {
      this.error = errorCopy(error);
    }
  }

  async toggleRelations() {
    if (!this.canConfigure) return;
    const useRelations = !this.useRelations;
    if (!this.active) {
      this.composer.setUseRelations(useRelations);
      return;
    }
    try {
      const updated = await this.ipc.call("chat.conversations.updateRelations", {
        id: this.active.id,
        useRelations,
      });
      runInAction(() => {
        this.active = { ...this.active!, ...updated };
        this.conversations = this.conversations.map((conversation) =>
          conversation.id === updated.id ? updated : conversation,
        );
      });
    } catch (error) {
      this.error = errorCopy(error);
    }
  }

  async mount() {
    this.mounted = true;
    this.loading = true;
    this.error = null;
    const revision = ++this.revision;
    try {
      const [conversations, summaries, stores, tools, accounts, adapters, servers, last] =
        await Promise.all([
          this.ipc.call("chat.conversations.list", undefined),
          this.ipc.call("providers.list", { capability: "text", enabled: true }),
          this.ipc.call("vectorStores.list", undefined),
          this.ipc
            .call("tools.list", { includeUnavailable: true })
            .catch((): ToolDto[] => this.tools),
          this.ipc.call("accounts.list", undefined).catch((): AccountDto[] => []),
          this.ipc.call("providers.adapters", undefined).catch((): AdapterDescriptorDto[] => []),
          this.ipc.call("mcp.list", undefined).catch((): McpServerDto[] => this.servers),
          this.lastConversation(),
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
        this.tools = tools;
        this.accounts = accounts;
        this.adapters = adapters;
        this.servers = servers;
        this.remembered = last;
        if (!this.composer.providerId && providers[0]) this.composer.selectProvider(providers[0]);
        this.loading = false;
      });
      this.stopContext?.();
      this.stopContext = reaction(
        () =>
          JSON.stringify([
            this.contextInput,
            this.active?.messages.length ?? 0,
            this.active?.updatedAt ?? 0,
          ]),
        () => void this.refreshContext(),
        { fireImmediately: true },
      );
      this.ready = true;
      const pending = this.pendingOpen;
      this.pendingOpen = null;
      if (this.run) this.subscribe();
      else if (pending !== null) {
        if (this.listed(pending)) await this.select(pending);
        else this.newChat();
      } else if (this.active) await this.select(this.active.id);
      else if (last !== null && this.listed(last)) await this.select(last);
    } catch (error) {
      runInAction(() => {
        if (revision === this.revision) {
          this.error = errorCopy(error);
          this.loading = false;
        }
      });
    }
  }

  async open(id: ConversationId) {
    if (!this.ready) {
      this.pendingOpen = id;
      return;
    }
    if (!this.listed(id)) {
      this.newChat();
      return;
    }
    await this.select(id);
    runInAction(() => {
      this.revealed += 1;
    });
  }

  private async lastConversation(): Promise<ConversationId | null> {
    try {
      const stored = await this.ipc.call("settings.get", { key: LAST_CONVERSATION_KEY });
      const parsed = ConversationId.safeParse(stored.value);
      return parsed.success ? parsed.data : null;
    } catch {
      return null;
    }
  }

  private remember(id: ConversationId | null) {
    if (this.remembered === id) return;
    this.remembered = id;
    void this.ipc
      .call("settings.set", { key: LAST_CONVERSATION_KEY, value: id })
      .catch(() => undefined);
  }

  private listed(id: ConversationId) {
    return this.conversations.some((conversation) => conversation.id === id);
  }

  async select(id: ConversationId) {
    if (this.generating) return;
    const revision = ++this.revision;
    this.loading = true;
    this.error = null;
    this.folderError = null;
    this.folderPrompt = null;
    try {
      const [active, live] = await Promise.all([
        this.ipc.call("chat.conversations.get", { id }),
        this.attachment(id),
      ]);
      void this.ipc
        .call("notifications.clear", { kind: "conversation", conversationId: id })
        .catch(() => undefined);
      runInAction(() => {
        if (revision !== this.revision) return;
        this.active = active;
        this.remember(active.id);
        const activeProvider = this.providers.find((provider) => provider.id === active.providerId);
        if (activeProvider) {
          this.composer.selectProvider(activeProvider);
          this.composer.setModel(active.modelId);
          this.composer.setTemperature(active.settings.temperature ?? this.composer.temperature);
        }
        this.liveText = "";
        this.liveReasoning = "";
        this.liveCitations = [];
        this.liveWebSources = [];
        this.liveSelections = [];
        this.liveToolCalls = [];
        this.outcome = null;
        this.retryText = "";
        this.pendingTurn = false;
        this.composer.setText("");
        if (live !== null && this.run === null) {
          this.restore(live);
          if (this.mounted) this.subscribe();
        }
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
    this.remember(null);
    this.loading = false;
    this.error = null;
    this.folderError = null;
    this.folderPrompt = null;
    this.outcome = null;
    this.liveText = "";
    this.liveReasoning = "";
    this.liveCitations = [];
    this.liveWebSources = [];
    this.liveSelections = [];
    this.liveToolCalls = [];
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
    this.runStartedAt = Date.now();
    this.error = null;
    this.outcome = null;
    this.retryText = text;
    this.liveText = "";
    this.liveCitations = [];
    this.liveWebSources = [];
    this.liveSelections = [];
    this.liveToolCalls = [];
    this.searchedStoreIds = [];
    this.pendingTurn = true;
    try {
      const id = await this.ensureConversation();
      const run = await this.ipc.call("chat.send", {
        conversationId: id,
        text,
        providerId: this.composer.providerId ?? undefined,
        modelId: this.composer.modelId,
        settings: { temperature: this.composer.temperature },
        selections: [...this.composer.selections],
        vendor: this.vendorOptions,
      });
      const sentSelections = [...this.composer.selections];
      runInAction(() => {
        this.run = run;
        this.lastSeq = -1;
        this.liveSelections = sentSelections;
        this.composer.setText("");
        this.composer.clearSelections();
        this.sending = false;
        if (this.mounted) this.subscribe();
      });
    } catch (error) {
      runInAction(() => {
        this.error = errorCopy(error);
        this.outcome = "failed";
        this.sending = false;
        this.runStartedAt = null;
        this.pendingTurn = false;
      });
    }
  }

  private async ensureConversation(): Promise<ConversationId> {
    if (this.active) return this.active.id;
    const conversation = await this.ipc.call("chat.conversations.create", {
      title: "Новый диалог",
      providerId: this.provider!.id,
      modelId: this.modelId,
      settings: { temperature: this.composer.temperature },
      systemPrompt: this.composer.systemPrompt,
      attachedStoreIds: [...this.composer.attachedStoreIds],
      useRelations: this.composer.useRelations,
      enabledTools: [...this.composer.enabledTools],
      retrievalMode: this.composer.retrievalMode,
    });
    runInAction(() => {
      this.active = { ...conversation, messages: [] };
      this.remember(conversation.id);
      this.conversations.unshift(conversation);
    });
    return conversation.id;
  }

  private applyConversation(conversation: ConversationDto) {
    this.conversations = this.conversations.map((item) =>
      item.id === conversation.id ? conversation : item,
    );
    if (this.active?.id === conversation.id) this.active = { ...this.active, ...conversation };
  }

  get canPickFolder() {
    return this.canChangeFolder && (this.active !== null || this.provider !== undefined);
  }

  async pickFolder() {
    if (!this.canPickFolder) return;
    this.folderBusy = true;
    this.folderError = null;
    try {
      const picked = await this.ipc.call("chat.pickFolder", undefined);
      if (picked.path === null) return;
      await this.bindFolder(picked.path);
    } catch (error) {
      runInAction(() => {
        this.folderError = folderErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.folderBusy = false;
      });
    }
  }

  async restoreFolderAccess() {
    const path = this.workingFolder;
    if (path === null || !this.canChangeFolder) return;
    this.folderBusy = true;
    this.folderError = null;
    try {
      await this.bindFolder(path);
    } catch (error) {
      runInAction(() => {
        this.folderError = folderErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.folderBusy = false;
      });
    }
  }

  private async bindFolder(path: string, grant?: FolderGrantScope) {
    const id = await this.ensureConversation();
    const result = await this.ipc.call("chat.bindFolder", {
      id,
      path,
      ...(grant === undefined ? {} : { grant }),
    });
    runInAction(() => {
      if (result.status === "needsGrant") {
        this.folderPrompt = { path: result.path };
        return;
      }
      this.folderPrompt = null;
      this.applyConversation(result.conversation);
    });
  }

  async confirmFolder(grant: FolderGrantScope) {
    const prompt = this.folderPrompt;
    if (prompt === null || this.folderBusy) return;
    this.folderBusy = true;
    this.folderError = null;
    try {
      await this.bindFolder(prompt.path, grant);
    } catch (error) {
      runInAction(() => {
        this.folderPrompt = null;
        this.folderError = folderErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.folderBusy = false;
      });
    }
  }

  cancelFolderPrompt() {
    this.folderPrompt = null;
  }

  async unbindFolder() {
    const active = this.active;
    if (!active || active.workingFolder === null || !this.canChangeFolder) return;
    this.folderBusy = true;
    this.folderError = null;
    try {
      const conversation = await this.ipc.call("chat.unbindFolder", { id: active.id });
      runInAction(() => {
        this.applyConversation(conversation);
      });
    } catch (error) {
      runInAction(() => {
        this.folderError = folderErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.folderBusy = false;
      });
    }
  }

  async answerFolder(answer: FolderAccessAnswer) {
    const request = this.folderRequest;
    if (request === null || this.answeringFolder) return;
    this.answeringFolder = true;
    try {
      await this.ipc.call("folders.answer", {
        id: request.runId,
        approvalId: request.id,
        answer,
      });
    } catch (error) {
      runInAction(() => {
        this.error = answerErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.folderRequests = this.folderRequests.filter((item) => item.id !== request.id);
        this.answeringFolder = false;
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
        this.liveWebSources = [];
        this.liveSelections = [];
        this.liveToolCalls = [];
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
    const selections = this.messageSelections(messageId);
    if (!(await this.truncateMessage(messageId))) return;
    this.composer.setText(content);
    this.composer.setSelections(selections);
    await this.send();
  }

  async editMessage(messageId: MessageId, content: string): Promise<boolean> {
    const text = content.trim();
    const selections = this.messageSelections(messageId);
    if (!text || !(await this.truncateMessage(messageId))) return false;
    this.composer.setText(text);
    this.composer.setSelections(selections);
    await this.send();
    return true;
  }

  private messageSelections(messageId: MessageId): DocumentSelectionDto[] {
    return [
      ...(this.active?.messages.find((message) => message.id === messageId)?.selections ?? []),
    ];
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

  async answerQuestion(requestId: string, answers: QuestionAnswerDto[]) {
    await this.resolveQuestion(requestId, (id) =>
      this.ipc.call("chat.answer", { id, requestId, answers }),
    );
  }

  async skipQuestion(requestId: string) {
    await this.resolveQuestion(requestId, (id) => this.ipc.call("chat.skip", { id, requestId }));
  }

  private async resolveQuestion(
    requestId: string,
    send: (runId: RunHandleDto["id"]) => Promise<void>,
  ) {
    const pending = this.questionFor(requestId);
    if (!pending || pending.busy || !this.run) return;
    pending.busy = true;
    try {
      await send(this.run.id);
      runInAction(() => {
        this.questions = this.questions.filter((item) => item.request.id !== requestId);
      });
    } catch (error) {
      runInAction(() => {
        pending.busy = false;
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

  private attachment(id: ConversationId): Promise<ChatAttachDto | null> {
    return this.ipc.call("chat.attach", { conversationId: id }).catch(() => null);
  }

  private restore(live: ChatAttachDto) {
    if (this.timer !== undefined) clearTimeout(this.timer);
    this.timer = undefined;
    this.buffer = "";
    this.run = live.run;
    this.runStartedAt = live.startedAt;
    this.lastSeq = live.seq ?? -1;
    this.contiguous = live.seq !== null;
    this.liveText = live.content;
    this.liveReasoning = live.reasoning;
    this.liveToolCalls = live.toolCalls;
    this.approvals = [];
    this.folderRequests = [];
    for (const request of live.approvals) this.admitApproval(request);
    this.questions = live.questions.map((request) => ({ request, busy: false }));
    this.outcome = null;
    this.stopping = false;
    if (this.active)
      this.active = {
        ...this.active,
        messages: this.active.messages.filter(
          (message) => !(message.partial && message.runId === live.run.id),
        ),
      };
  }

  private async reattach() {
    const run = this.run;
    const id = this.active?.id;
    if (!run || !id || this.attaching) return;
    this.attaching = true;
    const live = await this.attachment(id);
    runInAction(() => {
      this.attaching = false;
      const held = this.held;
      this.held = [];
      if (this.run?.id !== run.id) return;
      if (live?.run.id === run.id) this.restore(live);
      for (const event of held) if (this.run?.streamId === event.streamId) this.absorb(event);
      if (live?.run.id !== run.id && this.run?.id === run.id) this.end(null);
    });
  }

  private admitApproval(request: ApprovalRequestDto) {
    const run = this.run;
    if (!run) return;
    const folder = FolderApprovalDto.safeParse(request);
    if (folder.success && folder.data.runId === run.id) {
      if (!this.folderRequests.some((item) => item.id === folder.data.id))
        this.folderRequests.push(folder.data);
      return;
    }
    const id = request.id;
    const subject = request.subject;
    if (request.runId !== run.id || typeof id !== "string" || typeof subject !== "string") return;
    if (this.approvals.some((item) => item.id === id)) return;
    const files = FilesApprovalDto.safeParse(request);
    const process = ProcessApprovalDto.safeParse(request);
    this.approvals.push({
      id,
      subject,
      busy: false,
      changes: files.success ? files.data.changes : [],
      ...(process.success ? { process: process.data.process } : {}),
    });
  }

  private receive(event: RoutedEvent) {
    if (!this.run || event.streamId !== this.run.streamId) return;
    if (this.attaching) {
      this.held.push(event);
      return;
    }
    const missed = (event.gap ?? 0) > 0 || (this.contiguous && event.seq > this.lastSeq + 1);
    if (missed && event.type !== "end" && event.seq > this.lastSeq) {
      this.held.push(event);
      void this.reattach();
      return;
    }
    this.absorb(event);
  }

  private absorb(event: RoutedEvent) {
    if (!this.run || event.seq <= this.lastSeq) return;
    this.lastSeq = event.seq;
    this.contiguous = false;
    if (event.type === "approval") this.admitApproval(event.request);
    if (
      event.type === "question" &&
      event.request.runId === this.run.id &&
      !this.questions.some((question) => question.request.id === event.request.id)
    )
      this.questions.push({ request: event.request, busy: false });
    if (event.type === "token") {
      if (event.kind === "reasoning") this.liveReasoning += event.delta;
      else {
        this.buffer += event.delta;
        this.timer ??= setTimeout(this.flush, 32);
      }
    }
    if (event.type === "step" && event.step.domain === "tool-call") {
      const index = event.step.index;
      const call = ChatToolCallDto.safeParse(event.step.call);
      if (typeof index === "number" && call.success) {
        const calls = [...this.liveToolCalls];
        calls[Math.min(index, calls.length)] = call.data;
        this.liveToolCalls = calls;
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
              section: hit.section,
              page: hit.page,
              related: null,
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
          this.liveWebSources = message.data.webSources;
        }
      }
    }
    if (event.type === "end") this.end(event.outcome);
  }

  private end(outcome: RunOutcomeDto | null) {
    this.flush();
    this.outcome = outcome?.status ?? null;
    if (outcome?.status === "failed") this.error = outcome.message ?? GENERATION_FAILED;
    this.unsubscribe?.();
    this.unsubscribe = undefined;
    this.run = null;
    this.runStartedAt = null;
    this.approvals = [];
    this.questions = [];
    this.folderRequests = [];
    this.stopping = false;
    void this.refreshCompleted();
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
        this.liveWebSources = [];
        this.liveSelections = [];
        this.liveToolCalls = [];
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

  unmount() {
    this.release(false);
  }

  dispose() {
    this.release(true);
  }

  private release(full: boolean) {
    this.mounted = false;
    this.ready = false;
    ++this.revision;
    this.loading = false;
    this.flush();
    if (full || this.run === null) {
      this.unsubscribe?.();
      this.unsubscribe = undefined;
    }
    this.stopContext?.();
    this.stopContext = undefined;
    this.explorer.dispose();
    this.terminal.dispose();
  }
}

function errorCopy(error: unknown) {
  return error instanceof Error ? error.message : "Не удалось загрузить чат. Попробуйте ещё раз.";
}
