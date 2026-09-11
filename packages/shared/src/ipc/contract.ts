import { z } from "zod";
import { StreamId } from "../primitives/branded.js";
import { Timestamp } from "../primitives/time.js";
import { Json } from "../primitives/json.js";
import { SecretId } from "../primitives/branded.js";
import { CreateSecretInput } from "../secrets/CreateSecretInput.js";
import { UpdateSecretInput } from "../secrets/UpdateSecretInput.js";
import { SecretDto, SecretScope, SecretSummaryDto } from "../secrets/SecretDto.js";
import { SecretTypeKey } from "../secrets/registry.js";
import { SecretTypeSchema } from "../secrets/SecretTypeSchema.js";
import {
  CreateProviderInput,
  ProbeRequest,
  ProbeResultDto,
  ProviderDto,
  ProviderListFilter,
  ProviderRef,
  ProviderSummaryDto,
  SetDefaultModelInput,
  UpdateProviderInput,
} from "../providers/index.js";
import { ProviderId } from "../primitives/branded.js";
import { defineContract } from "./defineContract.js";
import { AccountDto, AccountLinkInput, AccountLinkResult, AccountRef } from "../accounts.js";
import { AdapterDescriptorDto } from "../ai.js";

export const SecretFilter = z.object({
  scope: SecretScope.optional(),
  type: SecretTypeKey.optional(),
  query: z.string().max(128).optional(),
});
export type SecretFilter = z.infer<typeof SecretFilter>;

export const SecretRef = z.object({ id: SecretId });
export type SecretRef = z.infer<typeof SecretRef>;

export const SettingKey = z
  .string()
  .min(1)
  .max(128)
  .regex(/^[a-z][a-z0-9]*(\.[a-z0-9]+)*$/);
export type SettingKey = z.infer<typeof SettingKey>;

export const contract = defineContract({
  "accounts.list": { input: z.void(), output: z.array(AccountDto) },
  "accounts.link": { input: AccountLinkInput, output: AccountLinkResult },
  "accounts.cancelLink": { input: AccountLinkInput, output: z.object({ cancelled: z.boolean() }) },
  "accounts.unlink": {
    input: AccountRef,
    output: AccountRef.extend({
      removed: z.literal(true),
      browserSessionPreserved: z.literal(true),
    }),
  },
  "accounts.refresh": { input: AccountRef, output: AccountDto },
  "system.ping": {
    input: z.object({ sentAt: Timestamp }),
    output: z.object({
      pong: z.literal(true),
      hostTime: Timestamp,
      roundTripHint: z.number(),
    }),
  },
  "system.demoStream": {
    input: z.object({ steps: z.number().int().min(1).max(100) }),
    output: z.object({ streamId: StreamId }),
  },
  "system.nativePing": {
    input: z.object({ text: z.string().max(1_000_000) }),
    output: z.object({ count: z.number().int().nonnegative() }),
  },
  "settings.get": {
    input: z.object({ key: SettingKey }),
    output: z.object({
      key: SettingKey,
      value: Json.optional(),
      updatedAt: Timestamp.optional(),
    }),
  },
  "settings.set": {
    input: z.object({ key: SettingKey, value: Json }),
    output: z.object({ key: SettingKey, value: Json, updatedAt: Timestamp }),
  },
  "secrets.types": {
    input: z.void(),
    output: z.array(SecretTypeSchema),
  },
  "secrets.list": {
    input: SecretFilter,
    output: z.array(SecretSummaryDto),
  },
  "secrets.get": {
    input: SecretRef,
    output: SecretDto,
  },
  "secrets.create": {
    input: CreateSecretInput,
    output: SecretDto,
  },
  "secrets.update": {
    input: UpdateSecretInput,
    output: SecretDto,
  },
  "secrets.remove": {
    input: SecretRef,
    output: z.object({ id: SecretId, removed: z.literal(true) }),
  },
  "providers.adapters": {
    input: z.void(),
    output: z.array(AdapterDescriptorDto),
  },
  "providers.list": {
    input: ProviderListFilter,
    output: z.array(ProviderSummaryDto),
  },
  "providers.get": {
    input: ProviderRef,
    output: ProviderDto,
  },
  "providers.create": {
    input: CreateProviderInput,
    output: ProviderDto,
  },
  "providers.update": {
    input: UpdateProviderInput,
    output: ProviderDto,
  },
  "providers.remove": {
    input: ProviderRef,
    output: z.object({ id: ProviderId, removed: z.literal(true) }),
  },
  "providers.probe": {
    input: ProbeRequest,
    output: ProbeResultDto,
  },
  "providers.setDefaultModel": {
    input: SetDefaultModelInput,
    output: ProviderDto,
  },
  "providers.refreshAll": {
    input: z.void(),
    output: z.array(ProviderSummaryDto),
  },
});

export type Contract = typeof contract;
