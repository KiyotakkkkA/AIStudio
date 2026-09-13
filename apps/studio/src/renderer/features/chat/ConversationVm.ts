import { makeAutoObservable } from "mobx";
import type { ProviderDto, ProviderId, VectorStoreId } from "@zvs/shared";

export default class ConversationVm {
  text = "";
  providerId: ProviderId | null = null;
  modelId = "";
  temperature = 0.7;
  systemPrompt = "";
  attachedStoreIds: VectorStoreId[] = [];

  constructor() {
    makeAutoObservable(this, {}, { autoBind: true });
  }

  setText(value: string) {
    this.text = value;
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
