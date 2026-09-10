import type {
  AccountDto,
  AdapterDescriptorDto,
  ModelDto,
  ModelId,
  ProviderCapability,
  ProviderDto,
  ProviderId,
  ProviderSummaryDto,
  SecretId,
  SecretSummaryDto,
} from "@zvs/shared";

export const OLLAMA = "0199bb11-1111-7111-8111-000000000001" as ProviderId;
export const EMBEDDER = "0199bb11-1111-7111-8111-000000000002" as ProviderId;
export const MODEL = "0199bb11-2222-7111-8111-000000000001" as ModelId;
export const OTHER_MODEL = "0199bb11-2222-7111-8111-000000000002" as ModelId;
export const SECRET = "0199bb11-3333-7111-8111-000000000001" as SecretId;

export const ADAPTERS: AdapterDescriptorDto[] = [
  {
    family: "openai-compatible",
    authModes: ["api"],
    streaming: true,
    liveModelList: true,
    embedding: true,
    image: false,
    honours: { temperature: true, topK: false, topP: true, maxOutputTokens: true },
    implemented: true,
  },
  {
    family: "anthropic",
    authModes: ["api"],
    streaming: true,
    liveModelList: true,
    embedding: false,
    image: false,
    honours: { temperature: true, topK: true, topP: true, maxOutputTokens: true },
    implemented: false,
  },
  {
    family: "qwen-web",
    authModes: ["account"],
    streaming: true,
    liveModelList: true,
    embedding: false,
    image: false,
    honours: { temperature: false, topK: false, topP: false, maxOutputTokens: false },
    implemented: true,
  },
];

export const SECRETS: SecretSummaryDto[] = [
  {
    id: SECRET,
    type: "ollama-cloud",
    name: "Ollama Cloud — личный",
    scope: "personal",
    hint: "osk_live_••••4f2a",
    tags: [],
    usageCount: 1,
    rotationStatus: "ok",
    rotatesAt: null,
    updatedAt: 1_700_000_000_000,
  },
];

export const ACCOUNTS: AccountDto[] = [];

export function summary(overrides: Partial<ProviderSummaryDto> & { id: ProviderId }) {
  return {
    kind: "ollama",
    adapter: "openai-compatible",
    authMode: "api",
    name: "Ollama Cloud",
    capabilities: ["text"] as ProviderCapability[],
    enabled: true,
    status: "ok",
    statusDetail: null,
    lastProbeAt: 1_700_000_000_000,
    lastLatencyMs: 412,
    modelCount: 2,
    defaultModelId: null,
    updatedAt: 1_700_000_000_000,
    ...overrides,
  } as ProviderSummaryDto;
}

export function model(overrides: Partial<ModelDto> & { id: ModelId }): ModelDto {
  return {
    providerId: OLLAMA,
    externalId: "gpt-oss:120b",
    displayName: "gpt-oss:120b",
    family: "gpt-oss",
    contextWindow: 128_000,
    maxOutput: 8192,
    sizeBytes: 69_000_000_000,
    capabilities: ["tools", "streaming"],
    available: true,
    unavailableReason: null,
    discoveredAt: 1_700_000_000_000,
    ...overrides,
  };
}

export function provider(overrides: Partial<ProviderDto> = {}): ProviderDto {
  return {
    ...summary({ id: OLLAMA }),
    baseUrl: "https://ollama.com/api",
    secretId: SECRET,
    secretName: "Ollama Cloud — личный",
    accountId: null,
    accountLabel: null,
    settings: { temperature: 0.4, maxOutputTokens: 2048 },
    adapterCapabilities: {
      family: "openai-compatible",
      authModes: ["api"],
      streaming: true,
      liveModelList: true,
      embedding: true,
      image: false,
      honours: { temperature: true, topK: false, topP: true, maxOutputTokens: true },
    },
    models: [model({ id: MODEL }), model({ id: OTHER_MODEL, externalId: "qwen3-coder:480b" })],
    ...overrides,
  };
}
