import { z } from "zod";
import { IntegrationId, SecretId } from "./primitives/branded.js";
import { Timestamp } from "./primitives/time.js";
import { FieldDescriptor } from "./secrets/SecretTypeSchema.js";
import { TOOL_KEY_PATTERN } from "./tools/ToolDto.js";

const trimmedName = (max: number) =>
  z
    .string()
    .refine((value) => value.trim().length > 0)
    .refine((value) => value.trim().length <= max);

const CapabilityName = z.string().min(1).max(128).regex(TOOL_KEY_PATTERN);
const count = z.number().int().nonnegative();

export const IntegrationKind = z.enum(["mail", "telegram", "git"]);
export type IntegrationKind = z.infer<typeof IntegrationKind>;

export const IntegrationStatus = z.enum([
  "disabled",
  "stopped",
  "starting",
  "running",
  "retrying",
  "failed",
]);
export type IntegrationStatus = z.infer<typeof IntegrationStatus>;

export const IntegrationActivityLevel = z.enum(["info", "warn", "error"]);
export type IntegrationActivityLevel = z.infer<typeof IntegrationActivityLevel>;

export const WorkerStatus = z.enum(["starting", "running", "retrying", "stopped", "failed"]);
export type WorkerStatus = z.infer<typeof WorkerStatus>;

export const IntegrationSecretRef = z.strictObject({ secretId: SecretId });
export type IntegrationSecretRef = z.infer<typeof IntegrationSecretRef>;

export const IntegrationValue = z.union([
  IntegrationSecretRef,
  z.boolean(),
  z.number(),
  z.string().max(4096),
]);
export type IntegrationValue = z.infer<typeof IntegrationValue>;

export const IntegrationConfigKey = z
  .string()
  .min(1)
  .max(64)
  .regex(/^[a-z][A-Za-z0-9]*$/);

export const IntegrationConfig = z
  .record(IntegrationConfigKey, IntegrationValue)
  .refine((config) => Object.keys(config).length <= 64);
export type IntegrationConfig = z.infer<typeof IntegrationConfig>;

export const IntegrationCountersDto = z.object({
  events: count,
  duplicates: count,
  runs: count,
  toolCalls: count,
  errors: count,
});
export type IntegrationCountersDto = z.infer<typeof IntegrationCountersDto>;

export const IntegrationAccountDto = z.object({
  id: IntegrationId,
  kind: IntegrationKind,
  name: trimmedName(120),
  enabled: z.boolean(),
  config: IntegrationConfig,
  status: IntegrationStatus,
  statusDetail: z.string().max(2000).nullable(),
  lastActivityAt: Timestamp.nullable(),
  counters: IntegrationCountersDto,
  createdAt: Timestamp,
  updatedAt: Timestamp,
});
export type IntegrationAccountDto = z.infer<typeof IntegrationAccountDto>;

export const IntegrationTriggerDto = z.object({
  name: CapabilityName,
  label: trimmedName(200),
  description: z.string().max(2000),
});
export type IntegrationTriggerDto = z.infer<typeof IntegrationTriggerDto>;

export const IntegrationToolDto = z.object({
  name: CapabilityName,
  label: trimmedName(200),
  description: z.string().max(2000),
  sideEffect: z.boolean(),
});
export type IntegrationToolDto = z.infer<typeof IntegrationToolDto>;

export const IntegrationAccountToolDto = IntegrationToolDto.extend({ key: CapabilityName });
export type IntegrationAccountToolDto = z.infer<typeof IntegrationAccountToolDto>;

export const IntegrationAccountDetailDto = IntegrationAccountDto.extend({
  triggers: z.array(IntegrationTriggerDto),
  tools: z.array(IntegrationAccountToolDto),
});
export type IntegrationAccountDetailDto = z.infer<typeof IntegrationAccountDetailDto>;

export const IntegrationKindDto = z.object({
  kind: IntegrationKind,
  label: trimmedName(200),
  fields: z.array(FieldDescriptor),
  triggers: z.array(IntegrationTriggerDto),
  tools: z.array(IntegrationToolDto),
});
export type IntegrationKindDto = z.infer<typeof IntegrationKindDto>;

export const IntegrationCheckDto = z.object({
  label: trimmedName(200),
  ok: z.boolean(),
  detail: z.string().max(2000).nullable(),
});
export type IntegrationCheckDto = z.infer<typeof IntegrationCheckDto>;

export const IntegrationTestResultDto = z.object({
  ok: z.boolean(),
  latencyMs: count,
  checks: z.array(IntegrationCheckDto),
  error: z.string().max(2000).nullable(),
});
export type IntegrationTestResultDto = z.infer<typeof IntegrationTestResultDto>;

export const IntegrationActivityDto = z.object({
  at: Timestamp,
  level: IntegrationActivityLevel,
  message: z.string().min(1).max(1000),
});
export type IntegrationActivityDto = z.infer<typeof IntegrationActivityDto>;

export const IntegrationRef = z.object({ id: IntegrationId });
export type IntegrationRef = z.infer<typeof IntegrationRef>;

export const IntegrationActivityInput = z.object({
  id: IntegrationId,
  limit: z.number().int().min(1).max(200).optional(),
});
export type IntegrationActivityInput = z.infer<typeof IntegrationActivityInput>;

export const CreateIntegrationInput = z.object({
  kind: IntegrationKind,
  name: trimmedName(120),
  config: IntegrationConfig.optional(),
  enabled: z.boolean().optional(),
});
export type CreateIntegrationInput = z.infer<typeof CreateIntegrationInput>;

export const UpdateIntegrationInput = z.object({
  id: IntegrationId,
  name: trimmedName(120).optional(),
  config: IntegrationConfig.optional(),
});
export type UpdateIntegrationInput = z.infer<typeof UpdateIntegrationInput>;

export const IntegrationRemoveOutput = z.object({ id: IntegrationId, removed: z.literal(true) });
export type IntegrationRemoveOutput = z.infer<typeof IntegrationRemoveOutput>;

export const WorkerKey = z
  .string()
  .min(1)
  .max(128)
  .regex(/^[a-z][a-z0-9.:_-]*$/);

export const WorkerDto = z.object({
  key: WorkerKey,
  label: trimmedName(200),
  status: WorkerStatus,
  restarts: count,
  detail: z.string().max(2000).nullable(),
  lastError: z.string().max(2000).nullable(),
  startedAt: Timestamp.nullable(),
  heartbeatAt: Timestamp.nullable(),
  retryAt: Timestamp.nullable(),
});
export type WorkerDto = z.infer<typeof WorkerDto>;

export const WorkerRef = z.object({ key: WorkerKey });
export type WorkerRef = z.infer<typeof WorkerRef>;
