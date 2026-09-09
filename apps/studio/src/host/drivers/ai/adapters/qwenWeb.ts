import { z } from "zod";
import type { ModelCapability } from "../../../data/schema/index.ts";
import type { AdapterCapabilities } from "../AdapterCapabilities.ts";
import type { DiscoveredModel } from "../ports.ts";
import { AccountFamilyAdapter, malformedPayload, type VendorObservation } from "./accountFamily.ts";

export const QWEN_MODELS_PATH = "/models/";

export const QWEN_MODELS_OBSERVATION: VendorObservation = {
  source: "https://chat.qwen.ai/api/v2/models/",
  observedAt: "2026-09-10",
};

export const QWEN_WEB_CAPABILITIES: AdapterCapabilities = {
  family: "qwen-web",
  authModes: ["account"],
  streaming: true,
  liveModelList: true,
  embedding: false,
  image: false,
  honours: { temperature: false, topK: false, topP: false, maxOutputTokens: false },
};

const positive = z.number().int().positive().nullish();
const flag = z.union([z.boolean(), z.number()]).nullish();

const QwenModelMeta = z.object({
  max_context_length: positive,
  max_generation_length: positive,
  modality: z.array(z.string()).nullish(),
  capabilities: z
    .object({ vision: flag, document: flag, video: flag, audio: flag, thinking: flag })
    .nullish(),
  abilities: z
    .object({ vision: flag, document: flag, video: flag, audio: flag, thinking: flag })
    .nullish(),
});

const QwenModelInfo = z.object({
  name: z.string().nullish(),
  is_active: z.boolean().nullish(),
  meta: QwenModelMeta.nullish(),
});

const QwenModelEntry = z.object({
  id: z.string(),
  name: z.string().nullish(),
  owned_by: z.string().nullish(),
  info: QwenModelInfo.nullish(),
});

const QwenModelList = z.array(QwenModelEntry);

export const QwenModelsPayload = z.object({
  success: z.boolean().nullish(),
  data: z.union([QwenModelList, z.object({ data: QwenModelList })]).nullish(),
});

type QwenEntry = z.infer<typeof QwenModelEntry>;

export function mapQwenModels(payload: unknown): DiscoveredModel[] | null {
  const parsed = QwenModelsPayload.safeParse(payload);
  if (!parsed.success) return null;
  const { success, data } = parsed.data;
  if (success === false || data == null) return null;
  const entries = Array.isArray(data) ? data : data.data;

  const seen = new Set<string>();
  const models: DiscoveredModel[] = [];
  for (const entry of entries) {
    const externalId = entry.id.trim();
    if (externalId.length === 0 || seen.has(externalId)) continue;
    if (entry.info?.is_active === false) continue;
    seen.add(externalId);
    models.push(toDiscoveredModel(externalId, entry));
  }
  return models;
}

export class QwenWebAdapter extends AccountFamilyAdapter {
  capabilities(): AdapterCapabilities {
    return QWEN_WEB_CAPABILITIES;
  }

  async listModels(signal: AbortSignal): Promise<DiscoveredModel[]> {
    const response = await this.request({ method: "GET", path: QWEN_MODELS_PATH }, signal);
    const models = mapQwenModels(this.decode(response.text, QWEN_MODELS_PATH));
    if (models === null) throw malformedPayload("qwen-web", QWEN_MODELS_PATH, this.logger);
    return models;
  }
}

function toDiscoveredModel(externalId: string, entry: QwenEntry): DiscoveredModel {
  const meta = entry.info?.meta;
  return {
    externalId,
    displayName: text(entry.name) ?? text(entry.info?.name) ?? externalId,
    family: text(entry.owned_by),
    contextWindow: meta?.max_context_length ?? null,
    maxOutput: meta?.max_generation_length ?? null,
    sizeBytes: null,
    capabilities: modelCapabilities(entry),
  };
}

function modelCapabilities(entry: QwenEntry): ModelCapability[] {
  const meta = entry.info?.meta;
  const capabilities: ModelCapability[] = ["streaming"];
  const modality = meta?.modality ?? [];
  if (enabled(meta?.capabilities?.vision, meta?.abilities?.vision) || modality.includes("image")) {
    capabilities.push("vision");
  }
  if (enabled(meta?.capabilities?.thinking, meta?.abilities?.thinking)) {
    capabilities.push("reasoning");
  }
  return capabilities;
}

function enabled(...values: readonly (boolean | number | null | undefined)[]): boolean {
  return values.some((value) => value === true || (typeof value === "number" && value > 0));
}

function text(value: string | null | undefined): string | null {
  const trimmed = value?.trim();
  return trimmed === undefined || trimmed.length === 0 ? null : trimmed;
}
