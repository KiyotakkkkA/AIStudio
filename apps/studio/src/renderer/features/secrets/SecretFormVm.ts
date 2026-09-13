import { makeAutoObservable } from "mobx";
import {
  buildFieldsSchema,
  configFieldsOf,
  schemaChip,
  secretFieldOf,
  type CreateSecretInput,
  type FieldDescriptor,
  type Json,
  type SecretDto,
  type SecretFields,
  type SecretId,
  type SecretScope,
  type SecretTypeKey,
  type SecretTypeSchema,
  type UpdateSecretInput,
} from "@zvs/shared";

export type FieldValue = string | boolean;

export const ROTATION_OPTIONS = [
  { value: "", label: "Никогда" },
  { value: "30", label: "Каждые 30 дней" },
  { value: "60", label: "Каждые 60 дней" },
  { value: "90", label: "Каждые 90 дней" },
  { value: "180", label: "Каждые 180 дней" },
  { value: "365", label: "Каждый год" },
] as const;

const ROTATION_DAYS = ROTATION_OPTIONS.map((option) => Number(option.value)).filter(
  (value) => value > 0,
);

interface Snapshot {
  readonly type: string;
  readonly name: string;
  readonly scope: SecretScope;
  readonly value: string;
  readonly fields: string;
  readonly tags: string;
  readonly note: string;
  readonly rotationDays: number | null;
}

export class SecretFormVm {
  readonly types: readonly SecretTypeSchema[];
  readonly secretId: SecretId | null;

  private readonly fallback: SecretTypeSchema;

  type: SecretTypeKey;
  name = "";
  scope: SecretScope = "personal";
  value = "";
  fieldValues: Record<string, FieldValue> = {};
  tags: string[] = [];
  note = "";
  rotationDays: number | null = null;
  errors: Record<string, string> = {};
  banner: string | null = null;

  private readonly initial: Snapshot;

  constructor(types: readonly SecretTypeSchema[], secret?: SecretDto | null) {
    const fallback = types[0];
    if (fallback === undefined) throw new Error("Реестр типов секретов пуст");
    this.types = types;
    this.fallback = fallback;
    this.secretId = secret?.id ?? null;
    this.type = (secret?.type ?? fallback.key) as SecretTypeKey;
    if (secret !== undefined && secret !== null) {
      this.name = secret.name;
      this.scope = secret.scope;
      this.tags = [...secret.tags];
      this.note = secret.note ?? "";
      this.rotationDays = rotationDaysOf(secret);
    }
    this.fieldValues = initialFieldValues(this.schema, secret?.fields);
    this.initial = this.snapshot();
    makeAutoObservable<SecretFormVm, "initial" | "types" | "fallback">(
      this,
      { initial: false, types: false, fallback: false },
      { autoBind: true },
    );
  }

  get isNew(): boolean {
    return this.secretId === null;
  }

  get schema(): SecretTypeSchema {
    return this.types.find((entry) => entry.key === this.type) ?? this.fallback;
  }

  get schemaChip(): string {
    return schemaChip(this.schema);
  }

  get credentialFields(): FieldDescriptor[] {
    return configFieldsOf(this.schema);
  }

  get secretField(): FieldDescriptor | undefined {
    return secretFieldOf(this.schema);
  }

  get valueRequired(): boolean {
    return this.isNew && this.secretField?.required === true;
  }

  get dirty(): boolean {
    const current = this.snapshot();
    return (
      current.type !== this.initial.type ||
      current.name !== this.initial.name ||
      current.scope !== this.initial.scope ||
      current.value !== this.initial.value ||
      current.fields !== this.initial.fields ||
      current.tags !== this.initial.tags ||
      current.note !== this.initial.note ||
      current.rotationDays !== this.initial.rotationDays
    );
  }

  setType(type: SecretTypeKey): void {
    if (type === this.type) return;
    this.type = type;
    this.value = "";
    this.fieldValues = initialFieldValues(this.schema);
    this.errors = withoutCredentialErrors(this.errors);
    this.banner = null;
  }

  setName(name: string): void {
    this.name = name;
    this.clearError("name");
  }

  setScope(scope: SecretScope): void {
    this.scope = scope;
  }

  setValue(value: string): void {
    this.value = value;
    this.clearError("value");
  }

  setField(key: string, value: FieldValue): void {
    this.fieldValues = { ...this.fieldValues, [key]: value };
    this.clearError(`fields.${key}`);
  }

  setTags(tags: string[]): void {
    this.tags = tags;
  }

  setNote(note: string): void {
    this.note = note;
  }

  setRotationDays(days: number | null): void {
    this.rotationDays = days;
  }

  setErrors(errors: Record<string, string>): void {
    this.errors = { ...errors };
  }

  setBanner(banner: string | null): void {
    this.banner = banner;
  }

  errorOf(key: string): string | undefined {
    return this.errors[key];
  }

  validate(): boolean {
    const errors: Record<string, string> = {};
    const name = this.name.trim();
    if (name.length === 0) errors.name = "Укажите название.";
    else if (name.length > 128) errors.name = "Не длиннее 128 символов.";
    if (this.valueRequired && this.value.trim().length === 0) {
      errors.value = "Укажите значение секрета.";
    }
    const parsed = buildFieldsSchema(this.schema).safeParse(this.fieldsPayload());
    if (!parsed.success) {
      for (const issue of parsed.error.issues) {
        const path = issue.path.map((segment) => String(segment)).join(".");
        errors[path.length === 0 ? "fields" : `fields.${path}`] = issueCopy(issue.code);
      }
    }
    this.errors = errors;
    return Object.keys(errors).length === 0;
  }

  fieldsPayload(): SecretFields {
    const payload: SecretFields = {};
    for (const field of this.credentialFields) {
      const raw = this.fieldValues[field.key];
      if (field.kind === "boolean") {
        payload[field.key] = raw === true;
        continue;
      }
      const text = typeof raw === "string" ? raw.trim() : "";
      if (text.length === 0) {
        if (field.required) payload[field.key] = "";
        continue;
      }
      payload[field.key] = field.kind === "number" ? Number(text) : text;
    }
    return payload;
  }

  toCreateInput(): CreateSecretInput {
    const note = this.note.trim();
    return {
      type: this.type,
      name: this.name.trim(),
      scope: this.scope,
      ...(this.value.length > 0 ? { value: this.value } : {}),
      fields: this.fieldsPayload(),
      tags: [...this.tags],
      ...(note.length > 0 ? { note } : {}),
      ...(this.rotationDays === null ? {} : { rotationDays: this.rotationDays }),
    };
  }

  toUpdateInput(): UpdateSecretInput {
    if (this.secretId === null) throw new Error("Форма не привязана к секрету");
    return {
      id: this.secretId,
      name: this.name.trim(),
      scope: this.scope,
      ...(this.value.length > 0 ? { value: this.value } : {}),
      fields: this.fieldsPayload(),
      tags: [...this.tags],
      note: this.note.trim(),
      rotationDays: this.rotationDays,
    };
  }

  private clearError(key: string): void {
    if (this.errors[key] === undefined) return;
    const rest: Record<string, string> = {};
    for (const [name, message] of Object.entries(this.errors)) {
      if (name !== key) rest[name] = message;
    }
    this.errors = rest;
  }

  private snapshot(): Snapshot {
    return {
      type: this.type,
      name: this.name,
      scope: this.scope,
      value: this.value,
      fields: JSON.stringify(this.fieldValues),
      tags: JSON.stringify(this.tags),
      note: this.note,
      rotationDays: this.rotationDays,
    };
  }
}

function initialFieldValues(
  schema: SecretTypeSchema,
  stored?: Readonly<Record<string, Json>>,
): Record<string, FieldValue> {
  const values: Record<string, FieldValue> = {};
  for (const field of configFieldsOf(schema)) {
    const saved = stored === undefined ? undefined : stored[field.key];
    if (field.kind === "boolean") {
      values[field.key] = typeof saved === "boolean" ? saved : field.default === true;
      continue;
    }
    if (saved !== undefined && saved !== null && typeof saved !== "object") {
      values[field.key] = String(saved);
      continue;
    }
    values[field.key] = field.default === undefined ? "" : String(field.default);
  }
  return values;
}

function withoutCredentialErrors(errors: Record<string, string>): Record<string, string> {
  const kept: Record<string, string> = {};
  for (const [key, message] of Object.entries(errors)) {
    if (key !== "value" && !key.startsWith("fields")) kept[key] = message;
  }
  return kept;
}

function rotationDaysOf(secret: SecretDto): number | null {
  if (secret.rotatesAt === null) return null;
  const days = Math.round((secret.rotatesAt - secret.updatedAt) / 86_400_000);
  return ROTATION_DAYS.includes(days) ? days : null;
}

function issueCopy(code: string): string {
  switch (code) {
    case "too_small":
      return "Обязательное поле.";
    case "invalid_format":
      return "Некорректный формат.";
    case "unrecognized_keys":
      return "Поле не описано схемой типа.";
    default:
      return "Некорректное значение.";
  }
}
