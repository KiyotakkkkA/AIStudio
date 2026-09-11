import type { DeltaKind } from "@zvs/shared";
import type { ModelCapability } from "../../data/schema/index.ts";
import type { AdapterCapabilities } from "./AdapterCapabilities.ts";

export const CHAT_ROLES = ["system", "user", "assistant"] as const;
export type ChatRole = (typeof CHAT_ROLES)[number];

export interface ChatMessage {
  readonly role: ChatRole;
  readonly content: string;
}

export interface GenerationSettings {
  readonly temperature?: number;
  readonly topK?: number;
  readonly topP?: number;
  readonly maxOutputTokens?: number;
}

export interface GenerateRequest extends GenerationSettings {
  readonly model: string;
  readonly messages: readonly ChatMessage[];
  readonly system?: string;
}

export const FINISH_REASONS = ["stop", "length", "cancelled", "unknown"] as const;
export type FinishReason = (typeof FINISH_REASONS)[number];

export interface TokenUsage {
  readonly promptTokens: number | null;
  readonly completionTokens: number | null;
  readonly totalTokens: number | null;
}

export interface GenerateResult {
  readonly model: string;
  readonly text: string;
  readonly reasoning: string | null;
  readonly finishReason: FinishReason;
  readonly usage: TokenUsage | null;
}

export interface TextDelta {
  readonly text: string;
  readonly kind: DeltaKind;
}

export interface DiscoveredModel {
  readonly isFree?: boolean | null;
  readonly noTraining?: boolean | null;
  readonly externalId: string;
  readonly displayName: string;
  readonly family: string | null;
  readonly contextWindow: number | null;
  readonly maxOutput: number | null;
  readonly sizeBytes: number | null;
  readonly capabilities: readonly ModelCapability[];
}

export interface TextGenerationDriver {
  capabilities(): AdapterCapabilities;
  generate(request: GenerateRequest, signal: AbortSignal): Promise<GenerateResult>;
  stream(request: GenerateRequest, signal: AbortSignal): AsyncIterable<TextDelta>;
  listModels(signal: AbortSignal): Promise<DiscoveredModel[]>;
}

export interface EmbeddingDriver {
  capabilities(): AdapterCapabilities;
  embed(texts: readonly string[], model: string, signal: AbortSignal): Promise<Float32Array[]>;
  dimensions(): number | null;
  listModels(signal: AbortSignal): Promise<DiscoveredModel[]>;
}

export interface ImageRequest {
  readonly model: string;
  readonly prompt: string;
  readonly count?: number;
  readonly width?: number;
  readonly height?: number;
}

export interface GeneratedImage {
  readonly mimeType: string;
  readonly data: Uint8Array;
}

export interface ImageDriver {
  capabilities(): AdapterCapabilities;
  generateImages(request: ImageRequest, signal: AbortSignal): Promise<GeneratedImage[]>;
  listModels(signal: AbortSignal): Promise<DiscoveredModel[]>;
}

export interface AiDriver {
  readonly text: TextGenerationDriver | null;
  readonly embedding: EmbeddingDriver | null;
  readonly image: ImageDriver | null;
}
