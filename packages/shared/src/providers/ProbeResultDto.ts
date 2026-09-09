import { z } from "zod";
import { AppErrorCode } from "../errors/AppErrorCode.js";
import { ProviderId } from "../primitives/branded.js";
import { Timestamp } from "../primitives/time.js";
import { ModelCapability, ProviderStatus } from "./enums.js";
import { ProviderConnectionInput, ProviderDto, ProviderRef } from "./ProviderDto.js";

export const DiscoveredModelDto = z.object({
  externalId: z.string().min(1),
  displayName: z.string(),
  family: z.string().nullable(),
  contextWindow: z.number().int().nonnegative().nullable(),
  maxOutput: z.number().int().nonnegative().nullable(),
  sizeBytes: z.number().int().nonnegative().nullable(),
  capabilities: z.array(ModelCapability),
});
export type DiscoveredModelDto = z.infer<typeof DiscoveredModelDto>;

const LatencyMs = z.number().int().nonnegative();

export const PROBE_OUTCOME_KINDS = [
  "ok",
  "ok-empty",
  "auth-failed",
  "account-not-linked",
  "session-expired",
  "unreachable",
  "rate-limited",
  "error",
] as const;
export type ProbeOutcomeKind = (typeof PROBE_OUTCOME_KINDS)[number];

export const ProbeOutcomeDto = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("ok"),
    latencyMs: LatencyMs,
    models: z.array(DiscoveredModelDto),
    live: z
      .boolean()
      .describe("false marks a curated list from a vendor with no discovery endpoint."),
  }),
  z.object({ kind: z.literal("ok-empty"), latencyMs: LatencyMs }),
  z.object({ kind: z.literal("auth-failed") }),
  z.object({ kind: z.literal("account-not-linked") }),
  z.object({ kind: z.literal("session-expired") }),
  z.object({ kind: z.literal("unreachable"), detail: z.string() }),
  z.object({
    kind: z.literal("rate-limited"),
    retryAfter: z.number().int().nonnegative().optional(),
  }),
  z.object({ kind: z.literal("error"), code: z.enum(AppErrorCode), detail: z.string() }),
]);
export type ProbeOutcomeDto = z.infer<typeof ProbeOutcomeDto>;

export const ProbeRequest = z.union([ProviderRef, z.object({ draft: ProviderConnectionInput })]);
export type ProbeRequest = z.infer<typeof ProbeRequest>;

export const ProbeResultDto = z.object({
  providerId: ProviderId.nullable().describe("null when an unsaved draft was probed."),
  outcome: ProbeOutcomeDto,
  status: ProviderStatus,
  statusDetail: z.string().nullable(),
  checkedAt: Timestamp,
  provider: ProviderDto.nullable(),
});
export type ProbeResultDto = z.infer<typeof ProbeResultDto>;
