import { makeAutoObservable } from "mobx";
import {
  CreateVectorStoreInput,
  type ModelDto,
  type ProviderSummaryDto,
  type VectorStoreDto,
  type UpdateVectorStoreInput,
} from "@zvs/shared";

export default class VectorStoreFormVm {
  name = "";
  description = "";
  embeddingProviderId = "";
  embeddingModelId = "";
  dimension = "";
  metric: "cosine" | "l2" | "dot" = "cosine";
  chunkSize = "256";
  chunkOverlap = "32";
  errors: Record<string, string> = {};

  constructor(
    readonly original: VectorStoreDto | null,
    readonly providers: readonly ProviderSummaryDto[],
    readonly modelsByProvider: ReadonlyMap<string, readonly ModelDto[]> = new Map(),
  ) {
    if (original) {
      this.name = original.name;
      this.description = original.description;
      this.embeddingProviderId = original.embeddingProviderId;
      this.embeddingModelId = original.embeddingModelId;
      this.dimension = String(original.dimension);
      this.metric = original.metric;
      this.chunkSize = String(original.chunkSize);
      this.chunkOverlap = String(original.chunkOverlap);
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
  set(
    field:
      | "name"
      | "description"
      | "embeddingProviderId"
      | "embeddingModelId"
      | "dimension"
      | "chunkSize"
      | "chunkOverlap",
    value: string,
  ) {
    if (field === "embeddingProviderId" && value !== this.embeddingProviderId) {
      this.embeddingModelId = "";
    }
    this[field] = value;
    this.errors = {};
  }
  setMetric(value: string) {
    if (value === "cosine" || value === "l2" || value === "dot") this.metric = value;
  }
  validate(): boolean {
    const result = CreateVectorStoreInput.safeParse(this.input());
    const errors: Record<string, string> = {};
    if (!result.success)
      for (const issue of result.error.issues) errors[String(issue.path[0])] = issue.message;
    if (!this.dimension.trim()) errors.dimension = "Укажите размерность явно.";
    if (!this.chunkSize.trim()) errors.chunkSize = "Укажите размер чанка.";
    if (!this.chunkOverlap.trim()) errors.chunkOverlap = "Укажите перекрытие, включая 0.";
    if (
      this.isNew &&
      !this.providers.some(
        (p) =>
          p.id === this.embeddingProviderId && p.enabled && p.capabilities.includes("embedding"),
      )
    )
      errors.embeddingProviderId = "Выберите включённый embedding-провайдер.";
    this.errors = errors;
    return Object.keys(errors).length === 0;
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
      ...(this.chunkLocked
        ? {}
        : { chunkSize: Number(this.chunkSize), chunkOverlap: Number(this.chunkOverlap) }),
    };
  }
}
