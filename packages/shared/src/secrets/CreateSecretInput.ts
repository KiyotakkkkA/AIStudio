import { z } from "zod";
import { SecretName, SecretScope } from "./SecretDto.js";

export const CreateSecretInput = z.object({
  name: SecretName,
  scope: SecretScope,
  value: z
    .string()
    .optional()
    .describe("The credential itself. The host refuses a secret without a non-blank value."),
  tags: z.array(z.string().min(1).max(32)).max(16).default([]),
  note: z.string().max(2000).optional(),
  rotationDays: z.number().int().positive().max(3650).optional(),
});
export type CreateSecretInput = z.infer<typeof CreateSecretInput>;
