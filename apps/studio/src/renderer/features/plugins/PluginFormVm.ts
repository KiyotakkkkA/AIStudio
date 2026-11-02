import { makeAutoObservable } from "mobx";
import type {
  FieldDescriptor,
  IntegrationAccountDto,
  IntegrationConfig,
  IntegrationId,
  IntegrationValue,
  SecretId,
  SecretSummaryDto,
} from "@zvs/shared";
import { hiddenKeys, type SignInType } from "./signInTypes";

export type PluginFieldValue = string | boolean;

export interface SecretOption {
  readonly value: string;
  readonly label: string;
}

export const NAME_MAX = 120;
export const NAME_REQUIRED = "Укажите название аккаунта.";
export const NAME_TOO_LONG = `Не длиннее ${String(NAME_MAX)} символов.`;
export const FIELD_REQUIRED = "Заполните обязательное поле.";
export const NUMBER_INVALID = "Укажите целое неотрицательное число.";

export function configKey(key: string): string {
  return `config.${key}`;
}

function initialValue(
  field: FieldDescriptor,
  stored: IntegrationValue | undefined,
): PluginFieldValue {
  if (field.kind === "boolean") {
    if (typeof stored === "boolean") return stored;
    return typeof field.default === "boolean" ? field.default : false;
  }
  if (field.kind === "secret") {
    return typeof stored === "object" && stored !== null ? stored.secretId : "";
  }
  if (typeof stored === "string" || typeof stored === "number") return String(stored);
  if (field.default === undefined || typeof field.default === "boolean") return "";
  return String(field.default);
}

export class PluginFormVm {
  readonly accountId: IntegrationId | null;
  readonly type: SignInType;

  name: string;
  values: Record<string, PluginFieldValue>;
  errors: Record<string, string> = {};

  private readonly fields: readonly FieldDescriptor[];
  private readonly stored: IntegrationConfig;
  private readonly initial: string;

  constructor(
    fields: readonly FieldDescriptor[],
    type: SignInType,
    account: Pick<IntegrationAccountDto, "id" | "name" | "config"> | null,
    defaultName: string,
  ) {
    this.fields = fields;
    this.type = type;
    this.accountId = account?.id ?? null;
    this.stored = account?.config ?? {};
    this.name = account?.name ?? defaultName;
    this.values = Object.fromEntries(
      fields.map((field) => [field.key, initialValue(field, this.stored[field.key])]),
    );
    this.initial = this.snapshot();
    makeAutoObservable<PluginFormVm, "fields" | "stored" | "initial">(
      this,
      { fields: false, stored: false, initial: false, accountId: false, type: false },
      { autoBind: true },
    );
  }

  get isNew(): boolean {
    return this.accountId === null;
  }

  get visibleFields(): FieldDescriptor[] {
    const hidden = hiddenKeys(this.type, this.values);
    return this.fields.filter((field) => !hidden.has(field.key));
  }

  get dirty(): boolean {
    return this.snapshot() !== this.initial;
  }

  isRequired(field: FieldDescriptor): boolean {
    return field.required || this.type.required.includes(field.key);
  }

  setName(name: string): void {
    this.name = name;
    this.clearError("name");
  }

  setValue(key: string, value: PluginFieldValue): void {
    this.values = { ...this.values, [key]: value };
    this.clearError(configKey(key));
  }

  setErrors(errors: Record<string, string>): void {
    this.errors = { ...errors };
  }

  errorOf(key: string): string | undefined {
    return this.errors[key];
  }

  validate(): boolean {
    const errors: Record<string, string> = {};
    const name = this.name.trim();
    if (name.length === 0) errors.name = NAME_REQUIRED;
    else if (name.length > NAME_MAX) errors.name = NAME_TOO_LONG;
    for (const field of this.visibleFields) {
      const value = this.values[field.key];
      if (field.kind === "boolean") continue;
      const text = typeof value === "string" ? value.trim() : "";
      if (text.length === 0) {
        if (this.isRequired(field)) errors[configKey(field.key)] = FIELD_REQUIRED;
        continue;
      }
      if (field.kind === "number" && !/^\d+$/.test(text)) {
        errors[configKey(field.key)] = NUMBER_INVALID;
      }
    }
    this.errors = errors;
    return Object.keys(errors).length === 0;
  }

  config(): IntegrationConfig {
    const visible = new Set(this.visibleFields.map((field) => field.key));
    const known = new Set(this.fields.map((field) => field.key));
    const config: IntegrationConfig = {};
    for (const [key, value] of Object.entries(this.stored)) {
      // Round-trip only keys the provider still describes: a removed field
      // must not bounce back into the config, the host refuses unknown keys.
      if (!visible.has(key) && known.has(key)) config[key] = value;
    }
    for (const field of this.visibleFields) {
      const value = this.values[field.key];
      if (field.kind === "boolean") {
        config[field.key] = value === true;
        continue;
      }
      const text = typeof value === "string" ? value.trim() : "";
      if (text.length === 0) continue;
      if (field.kind === "secret") config[field.key] = { secretId: text as SecretId };
      else if (field.kind === "number") config[field.key] = Number(text);
      else config[field.key] = text;
    }
    return { ...config, ...this.type.fixed };
  }

  private clearError(key: string): void {
    if (!(key in this.errors)) return;
    const next = { ...this.errors };
    delete next[key];
    this.errors = next;
  }

  private snapshot(): string {
    return JSON.stringify([this.name, this.values]);
  }
}

export function secretOptions(secrets: readonly SecretSummaryDto[]): SecretOption[] {
  return secrets
    .map((secret) => ({
      value: secret.id,
      label: secret.hint === null ? secret.name : `${secret.name} · ${secret.hint}`,
    }))
    .sort((left, right) => left.label.localeCompare(right.label, "ru-RU"));
}
