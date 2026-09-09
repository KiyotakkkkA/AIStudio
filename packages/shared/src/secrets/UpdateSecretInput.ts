import { z } from "zod";
import { Json } from "../primitives/json.js";
import { SecretId } from "../primitives/branded.js";
import { SecretName, SecretScope } from "./SecretDto.js";

export const UpdateSecretInput = z.object({
  id: SecretId,
  name: SecretName.optional(),
  scope: SecretScope.optional(),
  value: z
    .string()
    .min(1)
    .optional()
    .describe(
      "Absent means leave the stored credential exactly as it is. It never means clear it; " +
        "there is no way to clear a credential, only to replace it or delete the secret.",
    ),
  fields: z
    .record(z.string(), Json)
    .optional()
    .describe("When present, replaces the whole fields object and is validated as a whole."),
  tags: z.array(z.string().min(1).max(32)).max(16).optional(),
  note: z.string().max(2000).optional(),
  rotationDays: z
    .number()
    .int()
    .positive()
    .max(3650)
    .nullable()
    .optional()
    .describe("Absent leaves the reminder alone; null clears it."),
});
export type UpdateSecretInput = z.infer<typeof UpdateSecretInput>;
