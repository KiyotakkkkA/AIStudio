import { z } from "zod";
import { SecretId } from "../primitives/branded.js";
import { Timestamp } from "../primitives/time.js";
import { Json } from "../primitives/json.js";
import { SecretTypeKey } from "./registry.js";

export const SECRET_SCOPES = ["personal", "shared", "public"] as const;
export const SecretScope = z.enum(SECRET_SCOPES);
export type SecretScope = z.infer<typeof SecretScope>;

export const ROTATION_STATUSES = ["ok", "due-soon", "overdue"] as const;
export const RotationStatus = z.enum(ROTATION_STATUSES);
export type RotationStatus = z.infer<typeof RotationStatus>;

export const SecretName = z.string().min(1).max(128);
export type SecretName = z.infer<typeof SecretName>;

export const SecretSummaryDto = z.object({
  id: SecretId,
  type: SecretTypeKey,
  name: SecretName,
  scope: SecretScope,
  hint: z.string().nullable().describe("Masked tail of the stored value; never the value."),
  tags: z.array(z.string().min(1)),
  usageCount: z.number().int().nonnegative(),
  rotationStatus: RotationStatus,
  rotatesAt: Timestamp.nullable(),
  updatedAt: Timestamp,
});
export type SecretSummaryDto = z.infer<typeof SecretSummaryDto>;

export const SecretDto = SecretSummaryDto.extend({
  fields: z.record(z.string(), Json).describe("Non-secret schema fields only."),
  note: z.string().nullable(),
});
export type SecretDto = z.infer<typeof SecretDto>;
