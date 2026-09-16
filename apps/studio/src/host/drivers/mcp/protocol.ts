import { z } from "zod";

/** The version this client asks for. A server may answer with an older one it speaks. */
export const CLIENT_PROTOCOL_VERSION = "2025-06-18";

export const SUPPORTED_PROTOCOL_VERSIONS: readonly string[] = [
  "2025-06-18",
  "2025-03-26",
  "2024-11-05",
];

export const JsonRpcId = z.union([z.string().max(256), z.number()]);
export type JsonRpcId = z.infer<typeof JsonRpcId>;

export interface JsonRpcRequest {
  jsonrpc: "2.0";
  id: JsonRpcId;
  method: string;
  params?: Record<string, unknown>;
}

export interface JsonRpcNotification {
  jsonrpc: "2.0";
  method: string;
  params?: Record<string, unknown>;
}

export type JsonRpcOutgoing = JsonRpcRequest | JsonRpcNotification;

export const JsonRpcError = z.object({
  code: z.number(),
  message: z.string().max(4000).default(""),
  data: z.json().optional(),
});
export type JsonRpcError = z.infer<typeof JsonRpcError>;

export const JsonRpcResponse = z.object({
  jsonrpc: z.literal("2.0"),
  id: JsonRpcId,
  result: z.json().optional(),
  error: JsonRpcError.optional(),
});
export type JsonRpcResponse = z.infer<typeof JsonRpcResponse>;

export const JsonRpcIncomingNotification = z.object({
  jsonrpc: z.literal("2.0"),
  method: z.string().max(200),
  params: z.json().optional(),
});

/** A server may batch, answer, or push a notification or a request of its own down the pipe. */
export const JsonRpcIncoming = z.union([
  JsonRpcResponse,
  z.object({ jsonrpc: z.literal("2.0"), id: JsonRpcId, method: z.string().max(200) }).loose(),
  JsonRpcIncomingNotification,
]);

export const Implementation = z.object({
  name: z.string().max(200).default(""),
  title: z.string().max(200).optional(),
  version: z.string().max(64).default(""),
});

export const InitializeResult = z.object({
  protocolVersion: z.string().min(1).max(64),
  capabilities: z.record(z.string(), z.json()).default({}),
  serverInfo: Implementation.default({ name: "", version: "" }),
  instructions: z.string().max(20_000).optional(),
});
export type InitializeResult = z.infer<typeof InitializeResult>;

export const ToolAnnotations = z.object({
  title: z.string().max(200).optional(),
  readOnlyHint: z.boolean().optional(),
  destructiveHint: z.boolean().optional(),
  idempotentHint: z.boolean().optional(),
  openWorldHint: z.boolean().optional(),
});

export const McpToolDescriptor = z.object({
  name: z.string().min(1).max(200),
  title: z.string().max(200).optional(),
  description: z.string().max(8000).default(""),
  inputSchema: z.json().optional(),
  outputSchema: z.json().optional(),
  annotations: ToolAnnotations.optional(),
});
export type McpToolDescriptor = z.infer<typeof McpToolDescriptor>;

export const ListToolsResult = z.object({
  tools: z.array(McpToolDescriptor).default([]),
  nextCursor: z.string().max(2048).nullish(),
});

export const McpResourceDescriptor = z.object({
  uri: z.string().min(1).max(2048),
  name: z.string().max(200).default(""),
  title: z.string().max(200).optional(),
  description: z.string().max(8000).default(""),
  mimeType: z.string().max(200).nullish(),
});
export type McpResourceDescriptor = z.infer<typeof McpResourceDescriptor>;

export const ListResourcesResult = z.object({
  resources: z.array(McpResourceDescriptor).default([]),
  nextCursor: z.string().max(2048).nullish(),
});

export const McpPromptDescriptor = z.object({
  name: z.string().min(1).max(200),
  title: z.string().max(200).optional(),
  description: z.string().max(8000).default(""),
  arguments: z.array(z.json()).default([]),
});
export type McpPromptDescriptor = z.infer<typeof McpPromptDescriptor>;

export const ListPromptsResult = z.object({
  prompts: z.array(McpPromptDescriptor).default([]),
  nextCursor: z.string().max(2048).nullish(),
});

export const McpContent = z
  .object({ type: z.string().max(64).default("text"), text: z.string().nullish() })
  .loose();

export const CallToolResult = z.object({
  content: z.array(McpContent).default([]),
  structuredContent: z.json().optional(),
  isError: z.boolean().default(false),
});
export type CallToolResult = z.infer<typeof CallToolResult>;

const DESTRUCTIVE_VERBS =
  /(^|[._\-/])(delete|destroy|drop|remove|erase|purge|truncate|wipe|rm|unlink|revoke|uninstall|reset|prune)/i;

/**
 * TASK_031's rule applied to a name we did not choose: a server's own hints win when it sets
 * them, and a name that sounds like it removes something falls back to `off`.
 */
export function inferDestructive(tool: McpToolDescriptor): boolean {
  if (tool.annotations?.destructiveHint !== undefined) return tool.annotations.destructiveHint;
  if (tool.annotations?.readOnlyHint === true) return false;
  return DESTRUCTIVE_VERBS.test(tool.name);
}

export function inferReadOnly(tool: McpToolDescriptor): boolean {
  return tool.annotations?.readOnlyHint ?? false;
}
