import { makeAutoObservable } from "mobx";
import {
  CHAT_SELECTIONS_MAX,
  IMAGES_MAX,
  type ImageAttachmentDto,
  type ChatCommand,
  type DocumentSelectionDto,
  type IntegrationId,
  type ProviderDto,
  type ProviderId,
  type RetrievalMode,
  type ThinkingEffort,
  type VectorStoreId,
} from "@zvs/shared";

export default class ConversationVm {
  text = "";
  images: ImageAttachmentDto[] = [];
  providerId: ProviderId | null = null;
  modelId = "";
  temperature = 0.7;
  systemPrompt = "";
  attachedStoreIds: VectorStoreId[] = [];
  useRelations = false;
  retrievalMode: RetrievalMode = "hybrid";
  commands: ChatCommand[] = [];
  plugins: IntegrationId[] = [];
  selections: DocumentSelectionDto[] = [];
  effort: ThinkingEffort = "auto";
  webSearch = false;

  constructor() {
    makeAutoObservable(this, {}, { autoBind: true });
  }

  setImages(images: readonly ImageAttachmentDto[]) {
    this.images = images.slice(0, IMAGES_MAX);
  }
  removeImage(id: string) {
    this.images = this.images.filter((image) => image.id !== id);
  }
  clearImages() {
    this.images = [];
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
  addCommand(value: ChatCommand) {
    if (!this.commands.includes(value)) this.commands = [...this.commands, value];
  }
  removeCommand(value: ChatCommand) {
    this.commands = this.commands.filter((command) => command !== value);
  }
  addPlugin(value: IntegrationId) {
    if (!this.plugins.includes(value)) this.plugins = [...this.plugins, value];
  }
  removePlugin(value: IntegrationId) {
    this.plugins = this.plugins.filter((plugin) => plugin !== value);
  }
  get hasAttachments() {
    return this.images.length > 0 || this.commands.length > 0 || this.plugins.length > 0;
  }
  removeLastAttachment() {
    if (this.images.length > 0) this.images = this.images.slice(0, -1);
    else if (this.plugins.length > 0) this.plugins = this.plugins.slice(0, -1);
    else this.commands = this.commands.slice(0, -1);
  }
  clearAttachments() {
    this.commands = [];
    this.plugins = [];
    this.images = [];
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
