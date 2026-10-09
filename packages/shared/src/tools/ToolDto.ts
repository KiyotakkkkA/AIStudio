import { z } from "zod";
import { RunId, ToolId } from "../primitives/branded.js";
import { Json } from "../primitives/json.js";
import { Timestamp } from "../primitives/time.js";

export const ToolTier = z.enum(["auto", "ask", "off"]);
export type ToolTier = z.infer<typeof ToolTier>;

export const ToolSource = z.enum(["builtin", "mcp", "skill", "integration", "browser"]);
export type ToolSource = z.infer<typeof ToolSource>;

export const ToolKey = z
  .string()
  .trim()
  .min(1)
  .max(128)
  .regex(/^[a-z][a-zA-Z0-9]*(\.[a-zA-Z0-9_-]+)+$/);
export type ToolKey = z.infer<typeof ToolKey>;

export const ToolReferenceKind = z.enum(["scenario", "skill"]);
export type ToolReferenceKind = z.infer<typeof ToolReferenceKind>;

export const ToolReferenceDto = z.object({
  kind: ToolReferenceKind,
  refId: z.string().min(1).max(128),
  name: z.string().min(1).max(200),
});
export type ToolReferenceDto = z.infer<typeof ToolReferenceDto>;

export const ToolDto = z.object({
  id: ToolId,
  key: ToolKey,
  source: ToolSource,
  sourceRef: z.string().max(256).nullable(),
  displayName: z.string().min(1).max(200),
  description: z.string().max(4000),
  inputSchema: Json,
  readOnly: z.boolean(),
  destructive: z.boolean(),
  tier: ToolTier,
  enabled: z.boolean(),
  available: z.boolean(),
  lastUsedAt: Timestamp.nullable(),
  callCount: z.number().int().nonnegative(),
  registeredAt: Timestamp,
  updatedAt: Timestamp,
});
export type ToolDto = z.infer<typeof ToolDto>;

export const ToolDetailDto = ToolDto.extend({ references: z.array(ToolReferenceDto) });
export type ToolDetailDto = z.infer<typeof ToolDetailDto>;

export const ToolUsageDto = z.object({
  id: ToolId,
  key: ToolKey,
  callCount: z.number().int().nonnegative(),
  lastUsedAt: Timestamp.nullable(),
  references: z.array(ToolReferenceDto),
});
export type ToolUsageDto = z.infer<typeof ToolUsageDto>;

export const ToolRef = z.object({ id: ToolId });
export type ToolRef = z.infer<typeof ToolRef>;

export const ToolListFilter = z.object({
  source: ToolSource.optional(),
  tier: ToolTier.optional(),
  query: z.string().trim().max(128).optional(),
  includeUnavailable: z.boolean().default(true),
});
export type ToolListFilter = z.infer<typeof ToolListFilter>;
export type ToolListRequest = z.input<typeof ToolListFilter>;

export const SetToolTierInput = ToolRef.extend({ tier: ToolTier });
export type SetToolTierInput = z.infer<typeof SetToolTierInput>;

export const SetToolEnabledInput = ToolRef.extend({ enabled: z.boolean() });
export type SetToolEnabledInput = z.infer<typeof SetToolEnabledInput>;

export const ToolDefaultTier = z.enum(["inferred", "auto", "ask", "off"]);
export type ToolDefaultTier = z.infer<typeof ToolDefaultTier>;

export const ToolDefaultsDto = z.object({
  builtin: ToolDefaultTier.default("inferred"),
  mcp: ToolDefaultTier.default("inferred"),
  skill: ToolDefaultTier.default("inferred"),
  integration: ToolDefaultTier.default("inferred"),
  browser: ToolDefaultTier.default("inferred"),
});
export type ToolDefaultsDto = z.infer<typeof ToolDefaultsDto>;
export type ToolDefaultsRequest = z.input<typeof ToolDefaultsDto>;

export const ToolInvokeInput = ToolRef.extend({
  input: Json.default({}),
  confirmed: z.boolean().default(false),
});
export type ToolInvokeInput = z.infer<typeof ToolInvokeInput>;
export type ToolInvokeRequest = z.input<typeof ToolInvokeInput>;

export const ToolInvokeResultDto = z.object({
  runId: RunId,
  ok: z.boolean(),
  durationMs: z.number().int().nonnegative(),
  output: Json,
  error: z.string().nullable(),
});
export type ToolInvokeResultDto = z.infer<typeof ToolInvokeResultDto>;

export function toolNeedsConfirmation(tool: Pick<ToolDto, "tier" | "destructive">): boolean {
  return tool.destructive || tool.tier === "ask";
}
