import type { AppError } from "@zvs/shared";
import type { AdapterCapabilities } from "../AdapterCapabilities.ts";
import type { DiscoveredModel } from "../ports.ts";
import type { GenerateRequest, GenerateResult, TextDelta } from "../ports.ts";
import {
  AccountFamilyAdapter,
  failingStream,
  generationNotImplemented,
  type VendorObservation,
} from "./accountFamily.ts";

export const DEEPSEEK_MODELS_OBSERVATION: VendorObservation = {
  source: "https://chat.deepseek.com/ — model picker; no model-list endpoint is exposed",
  observedAt: "2026-09-10",
};

export const DEEPSEEK_GENERATION_OBSERVATION: VendorObservation = {
  source: "https://chat.deepseek.com/ — completion wire format not yet recorded",
  observedAt: "2026-09-10",
};

export const DEEPSEEK_WEB_CAPABILITIES: AdapterCapabilities = {
  family: "deepseek-web",
  authModes: ["account"],
  streaming: false,
  liveModelList: false,
  embedding: false,
  image: false,
  honours: { temperature: false, topK: false, topP: false, maxOutputTokens: false },
};

export const DEEPSEEK_CURATED_MODELS: readonly DiscoveredModel[] = [
  {
    externalId: "deepseek-chat",
    displayName: "DeepSeek",
    family: "deepseek",
    contextWindow: null,
    maxOutput: null,
    sizeBytes: null,
    capabilities: ["streaming"],
  },
  {
    externalId: "deepseek-reasoner",
    displayName: "DeepSeek (DeepThink)",
    family: "deepseek",
    contextWindow: null,
    maxOutput: null,
    sizeBytes: null,
    capabilities: ["streaming", "reasoning"],
  },
];

export class DeepSeekWebAdapter extends AccountFamilyAdapter {
  capabilities(): AdapterCapabilities {
    return DEEPSEEK_WEB_CAPABILITIES;
  }

  listModels(): Promise<DiscoveredModel[]> {
    return Promise.resolve(DEEPSEEK_CURATED_MODELS.map((model) => ({ ...model })));
  }

  generate(request: GenerateRequest): Promise<GenerateResult> {
    return Promise.reject(this.#pending(request));
  }

  stream(request: GenerateRequest): AsyncIterable<TextDelta> {
    return failingStream(this.#pending(request));
  }

  #pending(request: GenerateRequest): AppError {
    return generationNotImplemented(DEEPSEEK_WEB_CAPABILITIES.family, request.model);
  }
}
