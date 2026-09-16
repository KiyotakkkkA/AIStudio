import { z } from "zod";
import { McpServerId, SecretId } from "../primitives/branded.js";
import { Json } from "../primitives/json.js";
import { Timestamp } from "../primitives/time.js";

export const McpTransport = z.enum(["stdio", "http"]);
export type McpTransport = z.infer<typeof McpTransport>;

export const McpStartup = z.enum(["manual", "launch"]);
export type McpStartup = z.infer<typeof McpStartup>;

export const McpServerStatus = z.enum(["disabled", "stopped", "starting", "connected", "failed"]);
export type McpServerStatus = z.infer<typeof McpServerStatus>;

export const McpCapabilityKind = z.enum(["tool", "resource", "prompt"]);
export type McpCapabilityKind = z.infer<typeof McpCapabilityKind>;

/**
 * An environment or header value. A `secret` entry carries the reference only: the plaintext
 * is resolved at launch inside the host and never reaches a DTO, the database or the log.
 */
export const McpValue = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("literal"), value: z.string().max(4096) }),
  z.object({ kind: z.literal("secret"), secretId: SecretId }),
]);
export type McpValue = z.infer<typeof McpValue>;

export const McpEnvName = z
  .string()
  .trim()
  .min(1)
  .max(128)
  .regex(/^[A-Za-z_][A-Za-z0-9_]*$/);

export const McpHeaderName = z
  .string()
  .trim()
  .min(1)
  .max(128)
  .regex(/^[A-Za-z0-9!#$%&'*+.^_`|~-]+$/);

/** The first segment of every tool key this server contributes, per §17's `namespace.verb`. */
export const McpNamespace = z
  .string()
  .trim()
  .min(1)
  .max(48)
  .regex(/^[a-z][a-zA-Z0-9]*$/);

export const McpEnv = z.record(McpEnvName, McpValue);
export type McpEnv = z.infer<typeof McpEnv>;

export const McpHeaders = z.record(McpHeaderName, McpValue);
export type McpHeaders = z.infer<typeof McpHeaders>;

export const McpServerDto = z.object({
  id: McpServerId,
  name: z.string().min(1).max(120),
  namespace: McpNamespace,
  transport: McpTransport,
  command: z.string().max(512).nullable(),
  args: z.array(z.string().max(2048)),
  env: McpEnv,
  url: z.string().max(2048).nullable(),
  headers: McpHeaders,
  enabled: z.boolean(),
  startup: McpStartup,
  status: McpServerStatus,
  protocolVersion: z.string().max(64).nullable(),
  serverName: z.string().max(200).nullable(),
  serverVersion: z.string().max(64).nullable(),
  lastError: z.string().max(2000).nullable(),
  lastConnectedAt: Timestamp.nullable(),
  toolCount: z.number().int().nonnegative(),
  resourceCount: z.number().int().nonnegative(),
  promptCount: z.number().int().nonnegative(),
  createdAt: Timestamp,
  updatedAt: Timestamp,
});
export type McpServerDto = z.infer<typeof McpServerDto>;

export const McpCapabilityDto = z.object({
  id: z.uuid(),
  serverId: McpServerId,
  kind: McpCapabilityKind,
  name: z.string().min(1).max(200),
  title: z.string().max(200).nullable(),
  description: z.string().max(4000),
  /** The registry key a discovered tool was registered under; `null` for resources and prompts. */
  toolKey: z.string().max(128).nullable(),
  uri: z.string().max(2048).nullable(),
  mimeType: z.string().max(200).nullable(),
  schema: Json,
  readOnly: z.boolean(),
  destructive: z.boolean(),
  discoveredAt: Timestamp,
});
export type McpCapabilityDto = z.infer<typeof McpCapabilityDto>;

export const McpServerDetailDto = McpServerDto.extend({
  capabilities: z.array(McpCapabilityDto),
});
export type McpServerDetailDto = z.infer<typeof McpServerDetailDto>;

/** The mockup's test line: `handshake ok · 128 ms · protocol 2025-06-18 · server v0.8.1`. */
export const McpTestResultDto = z.object({
  ok: z.boolean(),
  latencyMs: z.number().int().nonnegative(),
  protocolVersion: z.string().max(64).nullable(),
  serverName: z.string().max(200).nullable(),
  serverVersion: z.string().max(64).nullable(),
  toolCount: z.number().int().nonnegative(),
  resourceCount: z.number().int().nonnegative(),
  promptCount: z.number().int().nonnegative(),
  error: z.string().max(2000).nullable(),
});
export type McpTestResultDto = z.infer<typeof McpTestResultDto>;

export const McpLogLineDto = z.object({
  at: Timestamp,
  level: z.enum(["debug", "info", "warn", "error"]),
  line: z.string().max(1000),
});
export type McpLogLineDto = z.infer<typeof McpLogLineDto>;

export const McpHealthDto = z.object({
  serverId: McpServerId,
  status: McpServerStatus,
  uptimeMs: z.number().int().nonnegative().nullable(),
  restarts: z.number().int().nonnegative(),
  calls: z.number().int().nonnegative(),
  errors: z.number().int().nonnegative(),
  medianLatencyMs: z.number().int().nonnegative().nullable(),
  recentLog: z.array(McpLogLineDto),
});
export type McpHealthDto = z.infer<typeof McpHealthDto>;

export const McpServerRef = z.object({ id: McpServerId });
export type McpServerRef = z.infer<typeof McpServerRef>;

const draft = {
  name: z.string().trim().min(1).max(120),
  namespace: McpNamespace.optional(),
  transport: McpTransport,
  command: z.string().trim().max(512).nullable().default(null),
  args: z.array(z.string().max(2048)).default([]),
  env: McpEnv.default({}),
  url: z.string().trim().url().max(2048).nullable().default(null),
  headers: McpHeaders.default({}),
  enabled: z.boolean().default(true),
  startup: McpStartup.default("manual"),
};

export const CreateMcpServerInput = z.object(draft);
export type CreateMcpServerInput = z.infer<typeof CreateMcpServerInput>;
export type CreateMcpServerRequest = z.input<typeof CreateMcpServerInput>;

export const UpdateMcpServerInput = z.object({ id: McpServerId }).extend({
  name: draft.name.optional(),
  namespace: McpNamespace.optional(),
  transport: McpTransport.optional(),
  command: z.string().trim().max(512).nullable().optional(),
  args: z.array(z.string().max(2048)).optional(),
  env: McpEnv.optional(),
  url: z.string().trim().url().max(2048).nullable().optional(),
  headers: McpHeaders.optional(),
  enabled: z.boolean().optional(),
  startup: McpStartup.optional(),
});
export type UpdateMcpServerInput = z.infer<typeof UpdateMcpServerInput>;

export const McpCallInput = z.object({
  serverId: McpServerId,
  tool: z.string().trim().min(1).max(200),
  arguments: z.record(z.string(), Json).default({}),
  timeoutMs: z.number().int().positive().max(600_000).optional(),
});
export type McpCallInput = z.infer<typeof McpCallInput>;

export const McpContentDto = z.object({
  type: z.string().max(64),
  text: z.string().nullable(),
  data: Json,
});
export type McpContentDto = z.infer<typeof McpContentDto>;

export const McpCallResultDto = z.object({
  content: z.array(McpContentDto),
  text: z.string(),
  structured: Json,
  isError: z.boolean(),
});
export type McpCallResultDto = z.infer<typeof McpCallResultDto>;
