import { makeAutoObservable } from "mobx";
import type {
  CreateSecretInput,
  SecretDto,
  SecretId,
  SecretScope,
  UpdateSecretInput,
} from "@zvs/shared";

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
  readonly name: string;
  readonly scope: SecretScope;
  readonly value: string;
  readonly tags: string;
  readonly note: string;
  readonly rotationDays: number | null;
}

export class SecretFormVm {
  readonly secretId: SecretId | null;
  readonly copied: boolean;

  name = "";
  scope: SecretScope = "personal";
  value = "";
  tags: string[] = [];
  note = "";
  rotationDays: number | null = null;
  errors: Record<string, string> = {};
  banner: string | null = null;

  private readonly initial: Snapshot;

  constructor(secret?: SecretDto | null, copy = false) {
    this.copied = copy && secret !== undefined && secret !== null;
    this.secretId = this.copied ? null : (secret?.id ?? null);
    if (secret !== undefined && secret !== null) {
      this.name = this.copied ? `${secret.name} (копия)` : secret.name;
      this.scope = secret.scope;
      this.tags = [...secret.tags];
      this.note = secret.note ?? "";
      this.rotationDays = rotationDaysOf(secret);
    }
    this.initial = this.snapshot();
    makeAutoObservable<SecretFormVm, "initial">(
      this,
      { initial: false, copied: false },
      { autoBind: true },
    );
  }

  get isNew(): boolean {
    return this.secretId === null;
  }

  get valueRequired(): boolean {
    return this.isNew;
  }

  get dirty(): boolean {
    if (this.copied) return true;
    const current = this.snapshot();
    return (
      current.name !== this.initial.name ||
      current.scope !== this.initial.scope ||
      current.value !== this.initial.value ||
      current.tags !== this.initial.tags ||
      current.note !== this.initial.note ||
      current.rotationDays !== this.initial.rotationDays
    );
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
    this.errors = errors;
    return Object.keys(errors).length === 0;
  }

  toCreateInput(): CreateSecretInput {
    const note = this.note.trim();
    return {
      name: this.name.trim(),
      scope: this.scope,
      ...(this.value.length > 0 ? { value: this.value } : {}),
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
      name: this.name,
      scope: this.scope,
      value: this.value,
      tags: JSON.stringify(this.tags),
      note: this.note,
      rotationDays: this.rotationDays,
    };
  }
}

function rotationDaysOf(secret: SecretDto): number | null {
  if (secret.rotatesAt === null) return null;
  const days = Math.round((secret.rotatesAt - secret.updatedAt) / 86_400_000);
  return ROTATION_DAYS.includes(days) ? days : null;
}
