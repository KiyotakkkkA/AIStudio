import type { DiscoveredModelDto, ModelCapability, ModelDto, ModelId } from "@zvs/shared";

export interface ModelRow {
  readonly isFree?: boolean | null;
  readonly noTraining?: boolean | null;
  readonly key: string;
  /** Null for a model returned by a draft probe: it has no row to point a default at yet. */
  readonly modelId: ModelId | null;
  readonly externalId: string;
  readonly displayName: string;
  readonly family: string | null;
  readonly contextWindow: number | null;
  readonly sizeBytes: number | null;
  readonly capabilities: readonly ModelCapability[];
  readonly available: boolean;
  readonly unavailableReason: string | null;
}

export function modelRows(models: readonly ModelDto[]): ModelRow[] {
  return models.map((model) => ({
    isFree: model.isFree ?? null,
    noTraining: model.noTraining ?? null,
    key: model.id,
    modelId: model.id,
    externalId: model.externalId,
    displayName: model.displayName,
    family: model.family,
    contextWindow: model.contextWindow,
    sizeBytes: model.sizeBytes,
    capabilities: model.capabilities,
    available: model.available,
    unavailableReason: model.unavailableReason,
  }));
}

export function discoveredRows(models: readonly DiscoveredModelDto[]): ModelRow[] {
  return models.map((model) => ({
    isFree: model.isFree ?? null,
    noTraining: model.noTraining ?? null,
    key: model.externalId,
    modelId: null,
    externalId: model.externalId,
    displayName: model.displayName,
    family: model.family,
    contextWindow: model.contextWindow,
    sizeBytes: model.sizeBytes,
    capabilities: model.capabilities,
    available: true,
    unavailableReason: null,
  }));
}
