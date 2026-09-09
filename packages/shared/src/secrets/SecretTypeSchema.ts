import { z } from "zod";
import type { Json } from "../primitives/json.js";

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

export const SecretTypeSchema = z.object({
  key: z
    .string()
    .min(1)
    .max(64)
    .regex(/^[a-z][a-z0-9]*(-[a-z0-9]+)*$/),
  version: z.number().int().positive(),
  label: z.string().min(1),
  fields: z.array(FieldDescriptor).min(1),
});
export type SecretTypeSchema = z.infer<typeof SecretTypeSchema>;

export function secretFieldOf(schema: SecretTypeSchema): FieldDescriptor | undefined {
  return schema.fields.find((field) => field.kind === "secret");
}

export function configFieldsOf(schema: SecretTypeSchema): FieldDescriptor[] {
  return schema.fields.filter((field) => field.kind !== "secret");
}

export function schemaChip(schema: SecretTypeSchema): string {
  return `${schema.key}@${schema.version}`;
}

function baseSchema(field: FieldDescriptor): z.ZodType {
  switch (field.kind) {
    case "text":
      return field.required ? z.string().min(1) : z.string();
    case "url":
      return z.url();
    case "number":
      return z.number();
    case "boolean":
      return z.boolean();
    case "select":
      return z.enum((field.options ?? []).map((option) => option.value));
    case "secret":
      return z.never();
  }
}

export type SecretFields = Record<string, Json>;

export function buildFieldsSchema(schema: SecretTypeSchema): z.ZodType<SecretFields> {
  const shape: Record<string, z.ZodType> = {};
  for (const field of configFieldsOf(schema)) {
    const base = baseSchema(field);
    if (field.required) shape[field.key] = base;
    else if (field.default === undefined) shape[field.key] = base.optional();
    else shape[field.key] = base.default(field.default);
  }
  return z.strictObject(shape) as unknown as z.ZodType<SecretFields>;
}
