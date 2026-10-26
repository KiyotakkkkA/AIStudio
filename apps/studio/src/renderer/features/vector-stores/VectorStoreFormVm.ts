import { makeAutoObservable } from "mobx";
import {
  ChatMinScore,
  CreateVectorStoreInput,
  DEFAULT_CHAT_MIN_SCORE,
  type CatalogueItemDto,
  type DeviceProfileDto,
  type ModelDto,
  type ProviderSummaryDto,
  type VectorOcrConfig,
  type VectorOcrLanguage,
  type VectorRerankConfig,
  type VectorSearchMode,
  type VectorStoreDto,
  type UpdateVectorStoreInput,
} from "@zvs/shared";
import {
  autofillPlan,
  embeddingCandidates,
  embeddingPrefixesFor,
  ocrCandidates,
  rerankCandidates,
} from "./autofill";

export interface VectorStoreFormContext {
  /** The Downloads catalogue, so the advanced sections offer what is actually on disk. */
  catalogue?: readonly CatalogueItemDto[];
  device?: DeviceProfileDto | null;
}

export default class VectorStoreFormVm {
  name = "";
  description = "";
  embeddingProviderId = "";
  embeddingModelId = "";
  dimension = "";
  metric: "cosine" | "l2" | "dot" = "cosine";
  chunkSize = "256";
  chunkOverlap = "32";
  queryPrefix = "";
  documentPrefix = "";
  searchMode: VectorSearchMode = "hybrid";
  chatMinScore = String(DEFAULT_CHAT_MIN_SCORE);
  rerankEnabled = false;
  rerankModelRef = "";
  rerankCandidatesCount = "50";
  ocrEnabled = false;
  ocrModelRef = "";
  ocrLanguage: VectorOcrLanguage = "auto";
  ocrMinCharsPerPage = "200";
  advancedOpen = true;
  autofillNote = "";
  errors: Record<string, string> = {};

  readonly catalogue: readonly CatalogueItemDto[];
  readonly device: DeviceProfileDto | null;

  constructor(
    readonly original: VectorStoreDto | null,
    readonly providers: readonly ProviderSummaryDto[],
    readonly modelsByProvider: ReadonlyMap<string, readonly ModelDto[]> = new Map(),
    context: VectorStoreFormContext = {},
    template: VectorStoreDto | null = null,
  ) {
    this.catalogue = context.catalogue ?? [];
    this.device = context.device ?? null;
    const source = original ?? template;
    if (source) {
      this.name = original ? source.name : `${source.name} (копия)`;
      this.description = source.description;
      this.embeddingProviderId = source.embeddingProviderId;
      this.embeddingModelId = source.embeddingModelId;
      this.dimension = String(source.dimension);
      this.metric = source.metric;
      this.chunkSize = String(source.chunkSize);
      this.chunkOverlap = String(source.chunkOverlap);
      this.queryPrefix = source.queryPrefix;
      this.documentPrefix = source.documentPrefix;
      this.searchMode = source.searchMode;
      this.chatMinScore = String(source.chatMinScore);
      this.applyRerank(source.rerank);
      this.applyOcr(source.ocr);
    }
    makeAutoObservable(this, {}, { autoBind: true });
  }

  get isNew() {
    return this.original === null;
  }
  get embeddingModels() {
    return this.modelsByProvider.get(this.embeddingProviderId) ?? [];
  }
  get chunkLocked() {
    return (
      this.original !== null &&
      (this.original.vectors > 0 ||
        this.original.documents > 0 ||
        this.original.lastIndexedAt !== null ||
        this.original.status === "broken")
    );
  }
  /** Installed rerankers, plus whatever the store already points at, so a saved choice stays visible. */
  get rerankModels() {
    return this.withCurrent(rerankCandidates(this.catalogue), this.rerankModelRef);
  }
  get ocrModels() {
    return this.withCurrent(ocrCandidates(this.catalogue), this.ocrModelRef);
  }
  set(
    field:
      | "name"
      | "description"
      | "embeddingProviderId"
      | "embeddingModelId"
      | "dimension"
      | "chunkSize"
      | "chunkOverlap"
      | "queryPrefix"
      | "documentPrefix"
      | "chatMinScore"
      | "rerankModelRef"
      | "rerankCandidatesCount"
      | "ocrModelRef"
      | "ocrMinCharsPerPage",
    value: string,
  ) {
    if (field === "embeddingProviderId" && value !== this.embeddingProviderId) {
      this.embeddingModelId = "";
    }
    if (field === "embeddingModelId" && value !== this.embeddingModelId && !this.chunkLocked) {
      const item = embeddingCandidates(this.catalogue).find(
        (candidate) => candidate.name.toLowerCase() === value.toLowerCase().split(":")[0],
      );
      const prefixes = embeddingPrefixesFor(value, item);
      this.queryPrefix = prefixes.queryPrefix;
      this.documentPrefix = prefixes.documentPrefix;
    }
    this[field] = value;
    this.errors = {};
  }
  setSearchMode(value: string) {
    if (value === "hybrid" || value === "vector") this.searchMode = value;
  }
  setMetric(value: string) {
    if (value === "cosine" || value === "l2" || value === "dot") this.metric = value;
  }
  setOcrLanguage(value: string) {
    if (value === "auto" || value === "rus" || value === "eng" || value === "rus+eng")
      this.ocrLanguage = value;
  }
  toggleAdvanced() {
    this.advancedOpen = !this.advancedOpen;
  }
  setRerankEnabled(enabled: boolean) {
    this.rerankEnabled = enabled;
    // Switching a stage on with one model installed should not need a second click.
    if (enabled && this.rerankModelRef === "")
      this.rerankModelRef = this.rerankModels[0]?.ref ?? "";
    this.errors = {};
  }
  setOcrEnabled(enabled: boolean) {
    this.ocrEnabled = enabled;
    if (enabled && this.ocrModelRef === "") this.ocrModelRef = this.ocrModels[0]?.ref ?? "";
    this.errors = {};
  }

  /**
   * Fills everything except the name and the description from this machine's profile and the
   * models it already has. Settings that are frozen after creation are left alone.
   */
  autofill() {
    const plan = autofillPlan({
      device: this.device,
      catalogue: this.catalogue,
      providers: this.providers,
      modelsByProvider: this.modelsByProvider,
      current: {
        embeddingProviderId: this.embeddingProviderId,
        embeddingModelId: this.embeddingModelId,
        dimension: this.dimension,
      },
    });
    if (this.isNew) {
      this.embeddingProviderId = plan.embeddingProviderId;
      this.embeddingModelId = plan.embeddingModelId;
      this.dimension = plan.dimension;
      this.metric = plan.metric;
    }
    if (!this.chunkLocked) {
      this.chunkSize = plan.chunkSize;
      this.chunkOverlap = plan.chunkOverlap;
      this.queryPrefix = plan.queryPrefix;
      this.documentPrefix = plan.documentPrefix;
    }
    this.applyRerank(plan.rerank);
    this.applyOcr(plan.ocr);
    this.advancedOpen = true;
    this.autofillNote = plan.note;
    this.errors = {};
  }

  validate(): boolean {
    const result = CreateVectorStoreInput.safeParse(this.input());
    const errors: Record<string, string> = {};
    if (!result.success)
      for (const issue of result.error.issues) errors[String(issue.path[0])] = issue.message;
    if (!this.dimension.trim()) errors.dimension = "Укажите размерность явно.";
    if (!this.chunkSize.trim()) errors.chunkSize = "Укажите размер фрагмента.";
    if (!this.chunkOverlap.trim()) errors.chunkOverlap = "Укажите перекрытие, включая 0.";
    if (
      this.isNew &&
      !this.providers.some(
        (p) =>
          p.id === this.embeddingProviderId && p.enabled && p.capabilities.includes("embedding"),
      )
    )
      errors.embeddingProviderId = "Выберите включённый embedding-провайдер.";
    if (!this.chatMinScore.trim() || !ChatMinScore.safeParse(this.chatMinScoreValue).success)
      errors.chatMinScore = "Укажите порог от 0 до 1.";
    if (this.rerankEnabled && this.rerankModelRef.trim() === "")
      errors.rerank = "Выберите установленную модель переранжирования.";
    if (this.ocrEnabled && this.ocrModelRef.trim() === "")
      errors.ocr = "Выберите установленную модель OCR.";
    this.errors = errors;
    return Object.keys(errors).length === 0;
  }
  private get chatMinScoreValue() {
    return Number(this.chatMinScore.replace(",", "."));
  }
  private rerank(): VectorRerankConfig {
    return {
      enabled: this.rerankEnabled,
      modelRef: this.rerankModelRef.trim(),
      candidates: Number(this.rerankCandidatesCount),
    };
  }
  private ocr(): VectorOcrConfig {
    return {
      enabled: this.ocrEnabled,
      modelRef: this.ocrModelRef.trim(),
      language: this.ocrLanguage,
      minCharsPerPage: Number(this.ocrMinCharsPerPage),
    };
  }
  private applyRerank(config: VectorRerankConfig) {
    this.rerankEnabled = config.enabled;
    this.rerankModelRef = config.modelRef;
    this.rerankCandidatesCount = String(config.candidates);
  }
  private applyOcr(config: VectorOcrConfig) {
    this.ocrEnabled = config.enabled;
    this.ocrModelRef = config.modelRef;
    this.ocrLanguage = config.language;
    this.ocrMinCharsPerPage = String(config.minCharsPerPage);
  }
  private withCurrent(items: readonly CatalogueItemDto[], ref: string) {
    if (ref === "" || items.some((item) => item.ref === ref)) return items;
    const known = this.catalogue.find((item) => item.ref === ref);
    return known ? [...items, known] : items;
  }
  private input() {
    return {
      name: this.name.trim(),
      description: this.description,
      embeddingProviderId: this.embeddingProviderId,
      embeddingModelId: this.embeddingModelId.trim(),
      dimension: Number(this.dimension),
      metric: this.metric,
      chunkSize: Number(this.chunkSize),
      chunkOverlap: Number(this.chunkOverlap),
      queryPrefix: this.queryPrefix,
      documentPrefix: this.documentPrefix,
      searchMode: this.searchMode,
      chatMinScore: this.chatMinScoreValue,
      rerank: this.rerank(),
      ocr: this.ocr(),
      backend: "lancedb" as const,
      indexType: "FLAT" as const,
    };
  }
  toCreateInput(): CreateVectorStoreInput {
    return CreateVectorStoreInput.parse(this.input());
  }
  toUpdateInput(): UpdateVectorStoreInput {
    if (!this.original) throw new Error("No store selected");
    return {
      id: this.original.id,
      name: this.name.trim(),
      description: this.description,
      rerank: this.rerank(),
      ocr: this.ocr(),
      searchMode: this.searchMode,
      chatMinScore: this.chatMinScoreValue,
      ...(this.chunkLocked
        ? {}
        : {
            chunkSize: Number(this.chunkSize),
            chunkOverlap: Number(this.chunkOverlap),
            queryPrefix: this.queryPrefix,
            documentPrefix: this.documentPrefix,
          }),
    };
  }
}
