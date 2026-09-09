import { z } from "zod";
import { Json } from "../primitives/json.js";
import { SecretTypeKey } from "./registry.js";
import { SecretName, SecretScope } from "./SecretDto.js";

export const CreateSecretInput = z.object({
  type: SecretTypeKey,
  name: SecretName,
  scope: SecretScope,
  value: z
    .string()
    .optional()
    .describe(
      "The credential itself. Required unless the type's secret field is optional; every " +
        "type in the registry currently requires one, and the host enforces that from the " +
        "schema rather than from this type.",
    ),
  fields: z.record(z.string(), Json).default({}),
  tags: z.array(z.string().min(1).max(32)).max(16).default([]),
  note: z.string().max(2000).optional(),
  rotationDays: z.number().int().positive().max(3650).optional(),
});
export type CreateSecretInput = z.infer<typeof CreateSecretInput>;
