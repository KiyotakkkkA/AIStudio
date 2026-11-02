import { z } from "zod";

export const FIELD_KINDS = ["secret", "text", "url", "number", "boolean", "select"] as const;

export const FieldKind = z.enum(FIELD_KINDS);
export type FieldKind = z.infer<typeof FieldKind>;

export const FieldOption = z.object({
  value: z.string().min(1),
  label: z.string().min(1),
});
export type FieldOption = z.infer<typeof FieldOption>;

export const FieldDescriptor = z.object({
  key: z
    .string()
    .min(1)
    .max(64)
    .regex(/^[a-z][A-Za-z0-9]*$/),
  label: z.string().min(1),
  kind: FieldKind,
  required: z.boolean(),
  placeholder: z.string().optional(),
  help: z.string().optional(),
  options: z.array(FieldOption).min(1).optional(),
  default: z.union([z.string(), z.number(), z.boolean()]).optional(),
});
export type FieldDescriptor = z.infer<typeof FieldDescriptor>;
