import { z } from "zod";
import { SecretTypeSchema } from "./SecretTypeSchema.js";

export const SECRET_TYPE_KEYS = ["ollama", "openrouter", "mistral", "custom"] as const;

export const SecretTypeKey = z.enum(SECRET_TYPE_KEYS);
export type SecretTypeKey = z.infer<typeof SecretTypeKey>;

export const SECRET_TYPE_REGISTRY: readonly SecretTypeSchema[] = [
  {
    key: "ollama",
    version: 1,
    label: "Ollama API key",
    fields: [
      {
        key: "apiKey",
        label: "API key",
        kind: "secret",
        required: true,
        placeholder: "osk_live_…",
      },
      {
        key: "baseUrl",
        label: "Base URL",
        kind: "url",
        required: false,
        default: "https://ollama.com/api",
      },
    ],
  },
  {
    key: "openrouter",
    version: 1,
    label: "OpenRouter API key",
    fields: [
      {
        key: "apiKey",
        label: "API key",
        kind: "secret",
        required: true,
        placeholder: "sk-or-v1-…",
      },
      {
        key: "baseUrl",
        label: "Base URL",
        kind: "url",
        required: false,
        default: "https://openrouter.ai/api/v1",
      },
      {
        key: "referer",
        label: "Referer",
        kind: "text",
        required: false,
        placeholder: "https://zvs.local",
        help: "Sent as HTTP-Referer so OpenRouter can attribute the traffic.",
      },
    ],
  },
  {
    key: "mistral",
    version: 1,
    label: "Mistral API key",
    fields: [
      {
        key: "apiKey",
        label: "API key",
        kind: "secret",
        required: true,
        placeholder: "…",
      },
      {
        key: "baseUrl",
        label: "Base URL",
        kind: "url",
        required: false,
        default: "https://api.mistral.ai/v1",
      },
    ],
  },
  {
    key: "custom",
    version: 1,
    label: "Иное",
    fields: [
      {
        key: "value",
        label: "Value",
        kind: "secret",
        required: true,
      },
      {
        key: "description",
        label: "Description",
        kind: "text",
        required: false,
        placeholder: "What this value is for",
      },
    ],
  },
];

const BY_KEY = new Map(SECRET_TYPE_REGISTRY.map((schema) => [schema.key, schema]));

export function findSecretTypeSchema(key: string): SecretTypeSchema | undefined {
  return BY_KEY.get(key);
}

export function isSecretTypeKey(key: string): key is SecretTypeKey {
  return BY_KEY.has(key);
}
