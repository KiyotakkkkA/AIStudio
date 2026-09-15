import type {
  CatalogueItemDto,
  DeviceProfileDto,
  DeviceTier,
  ModelDto,
  ProviderSummaryDto,
  VectorOcrConfig,
  VectorRerankConfig,
} from "@zvs/shared";

/**
 * Auto-fill picks every setting except the name and the description. Everything here is a
 * pure function of two facts the user already has — what this machine is, and what has
 * finished downloading — so the same inputs always produce the same form, and the reasoning
 * is testable without a renderer.
 */

/** Installed and usable. An `update` row is an older build of something that still works. */
export function installedItems(
  catalogue: readonly CatalogueItemDto[],
): readonly CatalogueItemDto[] {
  return catalogue.filter((item) => item.state === "installed" || item.state === "update");
}

function matches(item: CatalogueItemDto, pattern: RegExp): boolean {
  return (
    pattern.test(item.name) ||
    pattern.test(item.displayName) ||
    item.tags.some((tag) => pattern.test(tag))
  );
}

const RERANK = /rerank|переранж/i;
const OCR = /\bocr\b|-vl-|\bvl\b|vision|зрен/i;
/**
 * The multimodal projector ships beside a vision model and is tagged like one, but it is half a
 * model — the runtime finds it by name. Offering it as something to choose would only let the
 * user pick the wrong file.
 */
const PROJECTOR = /mmproj|проектор/i;

export function rerankCandidates(
  catalogue: readonly CatalogueItemDto[],
): readonly CatalogueItemDto[] {
  return installedItems(catalogue).filter((item) => matches(item, RERANK));
}

export function ocrCandidates(catalogue: readonly CatalogueItemDto[]): readonly CatalogueItemDto[] {
  return installedItems(catalogue).filter(
    (item) =>
      !matches(item, RERANK) &&
      !matches(item, PROJECTOR) &&
      matches(item, OCR) &&
      item.kind === "model",
  );
}

/** An embedding artefact on disk, used to recognise the provider model that serves it. */
export function embeddingCandidates(
  catalogue: readonly CatalogueItemDto[],
): readonly CatalogueItemDto[] {
  return installedItems(catalogue).filter((item) => item.kind === "embedding");
}

/**
 * Ollama serves `bge-m3` as `bge-m3:latest`; the catalogue knows it as `bge-m3`. Compare on
 * the part before the tag so a downloaded artefact recognises the model that serves it.
 */
function sameModel(externalId: string, catalogueName: string): boolean {
  const base = (value: string) => value.toLowerCase().split(":")[0]!.trim();
  return base(externalId) === base(catalogueName);
}

export interface ChunkPlan {
  chunkSize: number;
  chunkOverlap: number;
}

/**
 * Bigger chunks mean fewer, longer embedding calls and a heavier index. A weak machine is
 * better off with small chunks it can get through; a strong one keeps more context per chunk.
 */
export function chunkPlan(tier: DeviceTier): ChunkPlan {
  if (tier === "high") return { chunkSize: 768, chunkOverlap: 96 };
  if (tier === "medium") return { chunkSize: 512, chunkOverlap: 64 };
  return { chunkSize: 256, chunkOverlap: 32 };
}

export interface AutofillInput {
  device: DeviceProfileDto | null;
  catalogue: readonly CatalogueItemDto[];
  providers: readonly ProviderSummaryDto[];
  modelsByProvider: ReadonlyMap<string, readonly ModelDto[]>;
  /** What the form already holds, kept whenever it is still a valid choice. */
  current: { embeddingProviderId: string; embeddingModelId: string; dimension: string };
}

export interface AutofillPlan {
  embeddingProviderId: string;
  embeddingModelId: string;
  dimension: string;
  metric: "cosine";
  chunkSize: string;
  chunkOverlap: string;
  rerank: VectorRerankConfig;
  ocr: VectorOcrConfig;
  /** One line telling the user what the choice was based on. */
  note: string;
}

export function autofillPlan(input: AutofillInput): AutofillPlan {
  const { device, catalogue, providers, modelsByProvider, current } = input;
  const tier: DeviceTier = device?.tier ?? "low";

  const usable = providers.filter((provider) => provider.enabled);
  const provider =
    usable.find((candidate) => candidate.id === current.embeddingProviderId) ?? usable[0];
  const models = provider ? (modelsByProvider.get(provider.id) ?? []) : [];
  const embeddings = embeddingCandidates(catalogue);

  // Prefer the provider model that serves an embedding artefact this machine already has:
  // its dimension is then known rather than guessed.
  const downloaded = embeddings
    .map((item) => ({
      item,
      model: models.find((candidate) => sameModel(candidate.externalId, item.name)),
    }))
    .find((pair) => pair.model !== undefined);
  const kept = models.find((candidate) => candidate.externalId === current.embeddingModelId);
  const model = downloaded?.model ?? kept ?? models[0];
  const dimension =
    downloaded?.model !== undefined && downloaded.item.dimension !== undefined
      ? String(downloaded.item.dimension)
      : current.dimension.trim() !== ""
        ? current.dimension
        : "1024";

  const chunks = chunkPlan(tier);
  const reranker = rerankCandidates(catalogue)[0];
  const ocrModel = ocrCandidates(catalogue)[0];
  // OCR runs a vision model over every scanned page; it is only worth switching on where
  // there is hardware to run it. Reranking is a much cheaper cross-encoder pass.
  const ocrAffordable = tier === "high" || device?.gpu?.discrete === true;

  return {
    embeddingProviderId: provider?.id ?? "",
    embeddingModelId: model?.externalId ?? "",
    dimension,
    metric: "cosine",
    chunkSize: String(chunks.chunkSize),
    chunkOverlap: String(chunks.chunkOverlap),
    rerank: {
      enabled: reranker !== undefined && tier !== "low",
      modelRef: reranker?.ref ?? "",
      candidates: tier === "high" ? 80 : tier === "medium" ? 50 : 20,
    },
    ocr: {
      enabled: ocrModel !== undefined && ocrAffordable,
      modelRef: ocrModel?.ref ?? "",
      language: "auto",
      minCharsPerPage: 200,
    },
    note: describe(device, reranker, ocrModel, ocrAffordable),
  };
}

const TIER_LABELS: Record<DeviceTier, string> = {
  low: "базовый",
  medium: "средний",
  high: "высокий",
};

function describe(
  device: DeviceProfileDto | null,
  reranker: CatalogueItemDto | undefined,
  ocrModel: CatalogueItemDto | undefined,
  ocrAffordable: boolean,
): string {
  const parts: string[] = [];
  if (device) {
    const memory = Math.round(device.totalMemoryBytes / 1024 ** 3);
    parts.push(
      `Профиль устройства: ${TIER_LABELS[device.tier]} (${String(memory)} ГБ ОЗУ, ${String(device.cpuCores)} ядер${device.gpu ? `, ${device.gpu.vendor}` : ""})`,
    );
  } else parts.push("Профиль устройства недоступен — выбраны осторожные значения");
  parts.push(
    reranker === undefined
      ? "модель переранжирования не установлена"
      : `переранжирование: ${reranker.displayName}`,
  );
  parts.push(
    ocrModel === undefined
      ? "модель OCR не установлена"
      : ocrAffordable
        ? `OCR: ${ocrModel.displayName}`
        : `OCR выключен — ${ocrModel.displayName} требует дискретной видеокарты`,
  );
  return `${parts.join(" · ")}.`;
}
