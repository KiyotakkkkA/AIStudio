import type { ProviderDto } from "@zvs/shared";

export interface PreferredModelRef {
  readonly providerId: string;
  readonly modelId: string;
}

export interface PreferredModelView {
  readonly key: string;
  readonly providerName: string;
  readonly modelName: string;
  readonly account: boolean;
  readonly missing: boolean;
}

export function modelViews(
  models: readonly PreferredModelRef[],
  providers: readonly ProviderDto[],
): PreferredModelView[] {
  return models.map((entry) => {
    const provider = providers.find((item) => item.id === entry.providerId);
    const model = provider?.models.find(
      (item) => item.externalId === entry.modelId || item.id === entry.modelId,
    );
    return {
      key: `${entry.providerId}/${entry.modelId}`,
      providerName: provider?.name ?? entry.providerId,
      modelName: model === undefined ? entry.modelId : model.displayName || model.externalId,
      account: provider?.authMode === "account",
      missing: model === undefined || !model.available,
    };
  });
}

export interface PreferredModelList {
  readonly views: readonly PreferredModelView[];
  readonly providers: readonly ProviderDto[];
  readonly draftProviderId: string | null;
  readonly draftModelId: string | null;
  readonly draftProvider: ProviderDto | undefined;
  readonly canAdd: boolean;
  readonly full: boolean;
  readonly loaded: boolean;
  readonly error: string | null;
  selectDraftProvider(id: string): void;
  setDraftModel(id: string): void;
  addDraft(): Promise<void>;
  remove(index: number): Promise<void>;
  move(index: number, offset: -1 | 1): Promise<void>;
}
