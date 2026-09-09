import { z } from "zod";

export const PROVIDER_KINDS = [
  "ollama",
  "openrouter",
  "anthropic",
  "mistral",
  "openai-compatible",
] as const;
export const ProviderKind = z.enum(PROVIDER_KINDS);
export type ProviderKind = z.infer<typeof ProviderKind>;

export const PROVIDER_CAPABILITIES = ["text", "embedding", "image"] as const;
export const ProviderCapability = z.enum(PROVIDER_CAPABILITIES);
export type ProviderCapability = z.infer<typeof ProviderCapability>;

export const PROVIDER_STATUSES = ["unknown", "ok", "degraded", "failed", "needs-relink"] as const;
export const ProviderStatus = z.enum(PROVIDER_STATUSES);
export type ProviderStatus = z.infer<typeof ProviderStatus>;

export const MODEL_CAPABILITIES = ["tools", "vision", "streaming", "reasoning", "code"] as const;
export const ModelCapability = z.enum(MODEL_CAPABILITIES);
export type ModelCapability = z.infer<typeof ModelCapability>;
