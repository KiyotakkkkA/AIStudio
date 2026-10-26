import { makeAutoObservable } from "mobx";
import {
  CHAT_SELECTIONS_MAX,
  type ChatCommand,
  type DocumentSelectionDto,
  type ProviderDto,
  type ProviderId,
  type RetrievalMode,
  type ThinkingEffort,
  type VectorStoreId,
} from "@zvs/shared";

export default class ConversationVm {
  text = "";
  providerId: ProviderId | null = null;
  modelId = "";
  temperature = 0.7;
  systemPrompt = "";
  attachedStoreIds: VectorStoreId[] = [];
  useRelations = false;
  retrievalMode: RetrievalMode = "hybrid";
  command: ChatCommand | null = null;
  selections: DocumentSelectionDto[] = [];
  effort: ThinkingEffort = "auto";
  webSearch = false;

  constructor() {
    makeAutoObservable(this, {}, { autoBind: true });
  }

  setText(value: string) {
    this.text = value;
  }
  addSelection(selection: DocumentSelectionDto): boolean {
    const duplicate = this.selections.some(
      (item) =>
        item.documentId === selection.documentId &&
        item.page === selection.page &&
        item.start === selection.start &&
        item.end === selection.end,
    );
    if (duplicate) return true;
    if (this.selections.length >= CHAT_SELECTIONS_MAX) return false;
    this.selections = [...this.selections, selection];
    return true;
  }
  removeSelection(index: number) {
    this.selections = this.selections.filter((_, position) => position !== index);
  }
  clearSelections() {
    this.selections = [];
  }
  setSelections(selections: readonly DocumentSelectionDto[]) {
    this.selections = selections.slice(0, CHAT_SELECTIONS_MAX);
  }
  setModel(value: string) {
    this.modelId = value;
  }
  setTemperature(value: number) {
    this.temperature = Math.min(2, Math.max(0, value));
  }
  setSystemPrompt(value: string) {
    this.systemPrompt = value;
  }
  setUseRelations(value: boolean) {
    this.useRelations = value;
  }
  setRetrievalMode(value: RetrievalMode) {
    this.retrievalMode = value;
  }
  setEffort(value: ThinkingEffort) {
    this.effort = value;
  }
  setWebSearch(value: boolean) {
    this.webSearch = value;
  }
  setCommand(value: ChatCommand | null) {
    this.command = value;
  }
  selectProvider(provider: ProviderDto) {
    this.providerId = provider.id;
    this.modelId =
      provider.models.find((model) => model.available && model.id === provider.defaultModelId)
        ?.externalId ??
      provider.models.find((model) => model.available)?.externalId ??
      "";
    this.temperature = provider.settings.temperature ?? 0.7;
  }
  toggleStore(id: VectorStoreId) {
    this.attachedStoreIds = this.attachedStoreIds.includes(id)
      ? this.attachedStoreIds.filter((value) => value !== id)
      : [...this.attachedStoreIds, id];
  }
}
