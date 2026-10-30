import { z } from "zod";
import { AdapterCapabilitiesDto, AdapterFamily, AuthMode } from "../ai.js";
import { AccountId, ModelId, ProviderId, SecretId } from "../primitives/branded.js";
import { Timestamp } from "../primitives/time.js";
import { ModelCapability, ProviderCapability, ProviderKind, ProviderStatus } from "./enums.js";

export const ProviderName = z.string().trim().min(1).max(128);
export type ProviderName = z.infer<typeof ProviderName>;

export const BaseUrl = z.url().max(2048);
export type BaseUrl = z.infer<typeof BaseUrl>;

export const ProviderSettingsDto = z.object({
  openaiMode: z.enum(["direct", "codex"]).optional(),
  // External vendor IDs also identify models selected from an unsaved draft probe.
  selectedModelIds: z.array(z.string().min(1)).optional(),
  temperature: z.number().min(0).max(2).optional(),
  topK: z.number().int().min(1).max(4096).optional(),
  topP: z.number().min(0).max(1).optional(),
  maxOutputTokens: z.number().int().min(1).max(1_000_000).optional(),
  timeoutSeconds: z.number().int().min(1).max(600).optional(),
});
export type ProviderSettingsDto = z.infer<typeof ProviderSettingsDto>;

export const ProviderUsageWindowDto = z.object({
  label: z.string(),
  usedPercent: z.number().min(0).max(100),
  resetsAt: Timestamp.nullable(),
});
export type ProviderUsageWindowDto = z.infer<typeof ProviderUsageWindowDto>;

export const ProviderUsageDto = z.object({
  source: z.string(),
  windows: z.array(ProviderUsageWindowDto),
  detail: z.string().nullable(),
  checkedAt: Timestamp,
});
export type ProviderUsageDto = z.infer<typeof ProviderUsageDto>;

export const ModelDto = z.object({
  isFree: z.boolean().nullable().optional(),
  noTraining: z.boolean().nullable().optional(),
  id: ModelId,
  providerId: ProviderId,
  externalId: z.string().min(1),
  displayName: z.string(),
  family: z.string().nullable(),
  contextWindow: z.number().int().nonnegative().nullable(),
  maxOutput: z.number().int().nonnegative().nullable(),
  sizeBytes: z.number().int().nonnegative().nullable(),
  capabilities: z.array(ModelCapability),
  available: z.boolean(),
  unavailableReason: z.string().nullable(),
  discoveredAt: Timestamp,
});
export type ModelDto = z.infer<typeof ModelDto>;

export const ProviderSummaryDto = z.object({
  id: ProviderId,
  kind: ProviderKind,
  adapter: AdapterFamily,
  authMode: AuthMode,
  name: ProviderName,
  capabilities: z.array(ProviderCapability),
  enabled: z.boolean(),
  status: ProviderStatus,
  statusDetail: z.string().nullable(),
  lastProbeAt: Timestamp.nullable(),
  lastLatencyMs: z.number().int().nonnegative().nullable(),
  modelCount: z.number().int().nonnegative(),
  defaultModelId: ModelId.nullable(),
  updatedAt: Timestamp,
});
export type ProviderSummaryDto = z.infer<typeof ProviderSummaryDto>;

export const ProviderDto = ProviderSummaryDto.extend({
  baseUrl: z.string(),
  secretId: SecretId.nullable(),
  secretName: z
    .string()
    .nullable()
    .describe("Display name of the referenced secret; never its value."),
  accountId: AccountId.nullable(),
  accountLabel: z
    .string()
    .nullable()
    .describe("Masked vendor identity; never a token or a cookie."),
  settings: ProviderSettingsDto,
  adapterCapabilities: AdapterCapabilitiesDto,
  models: z.array(ModelDto),
});
export type ProviderDto = z.infer<typeof ProviderDto>;

export const ProviderConnectionInput = z.object({
  kind: ProviderKind,
  adapter: AdapterFamily.default("openai-compatible"),
  authMode: AuthMode.default("api"),
  baseUrl: BaseUrl,
  secretId: SecretId.nullable().default(null),
  accountId: AccountId.nullable().default(null),
  capabilities: z.array(ProviderCapability).min(1),
  settings: ProviderSettingsDto.default({}),
});
export type ProviderConnectionInput = z.infer<typeof ProviderConnectionInput>;

export const CreateProviderInput = ProviderConnectionInput.extend({
  name: ProviderName,
  enabled: z.boolean().default(true),
});
export type CreateProviderInput = z.infer<typeof CreateProviderInput>;

export const UpdateProviderInput = z.object({
  id: ProviderId,
  name: ProviderName.optional(),
  adapter: AdapterFamily.optional(),
  authMode: AuthMode.optional(),
  baseUrl: BaseUrl.optional(),
  secretId: SecretId.nullable().optional(),
  accountId: AccountId.nullable().optional(),
  capabilities: z.array(ProviderCapability).min(1).optional(),
  settings: ProviderSettingsDto.optional(),
  enabled: z.boolean().optional(),
});
export type UpdateProviderInput = z.infer<typeof UpdateProviderInput>;

export const ProviderListFilter = z.object({
  capability: ProviderCapability.optional(),
  enabled: z.boolean().optional(),
});
export type ProviderListFilter = z.infer<typeof ProviderListFilter>;

export const ProviderRef = z.object({ id: ProviderId });
export type ProviderRef = z.infer<typeof ProviderRef>;

export const SetDefaultModelInput = z.object({ id: ProviderId, modelId: ModelId.nullable() });
export type SetDefaultModelInput = z.infer<typeof SetDefaultModelInput>;
