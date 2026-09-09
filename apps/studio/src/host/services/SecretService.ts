import {
  AppError,
  AppErrorCode,
  timestampNow,
  type Json,
  type SecretDto,
  type SecretId,
  type SecretSummaryDto,
  type SecretTypeKey,
  type Timestamp,
} from "@zvs/shared";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import type { SecretFilter, SecretPatch } from "../data/repositories/index.ts";
import type {
  SecretConsumerKind,
  SecretScope,
  SecretSummary as SecretRow,
} from "../data/schema/index.ts";
import type { Logger } from "../platform/logger.ts";
import type { CryptoService } from "./CryptoService.ts";
import { computeHint, rotationDueAt, rotationStatus, type RotationStatus } from "./secretPolicy.ts";

export type SecretConsumer = {
  kind: SecretConsumerKind;
  id: string;
};

export interface SecretSummary {
  readonly id: string;
  readonly type: string;
  readonly name: string;
  readonly scope: SecretScope;
  readonly cipherVersion: number;
  readonly hint: string | null;
  readonly usageCount: number;
  readonly fields: Readonly<Record<string, Json>>;
  readonly tags: readonly string[];
  readonly note: string | null;
  readonly rotationDays: number | null;
  readonly rotatesAt: Timestamp | null;
  readonly rotation: RotationStatus;
  readonly createdAt: Timestamp;
  readonly updatedAt: Timestamp;
}

export interface SecretCreateInput {
  type: string;
  name: string;
  scope: SecretScope;
  value?: string;
  fields?: Readonly<Record<string, Json>>;
  tags?: readonly string[];
  note?: string | null;
  rotationDays?: number | null;
  rotatesAt?: Timestamp | null;
}

export interface SecretUpdateInput {
  name?: string;
  scope?: SecretScope;
  value?: string;
  fields?: Readonly<Record<string, Json>>;
  tags?: readonly string[];
  note?: string | null;
  rotationDays?: number | null;
  rotatesAt?: Timestamp | null;
}

export interface SecretServiceOptions {
  data: UnitOfWork;
  crypto: CryptoService;
  logger?: Logger;
  clock?: () => number;
}

export class SecretService {
  readonly #data: UnitOfWork;
  readonly #crypto: CryptoService;
  readonly #logger?: Logger;
  readonly #clock: () => number;

  constructor(options: SecretServiceOptions) {
    this.#data = options.data;
    this.#crypto = options.crypto;
    this.#logger = options.logger;
    this.#clock = options.clock ?? Date.now;
  }

  list(filter: SecretFilter = {}): SecretSummary[] {
    const now = timestampNow(this.#clock);
    const usage = this.#data.repositories.secrets.countUsageBySecret();
    return this.#data.repositories.secrets
      .listSummaries(filter)
      .map((row) => toSummary(row, now, usage.get(row.id) ?? 0));
  }

  get(id: string): SecretSummary {
    const row = this.#require(id);
    return toSummary(row, timestampNow(this.#clock), this.#countUsage(id));
  }

  create(input: SecretCreateInput): SecretSummary {
    const name = requireText(input.name, "name");
    const value = input.value === undefined ? undefined : requireValue(input.value);
    const now = timestampNow(this.#clock);
    const created = this.#data.repositories.secrets.create({
      type: input.type,
      name,
      scope: input.scope,
      cipher: value === undefined ? null : this.#crypto.encrypt(value),
      cipherVersion: 1,
      hint: value === undefined ? null : computeHint(value),
      fields: JSON.stringify(input.fields ?? {}),
      tags: JSON.stringify(input.tags ?? []),
      note: input.note ?? null,
      rotationDays: input.rotationDays ?? null,
      rotatesAt: input.rotatesAt ?? rotationDueAt(input.rotationDays, now),
      createdAt: now,
      updatedAt: now,
    });
    this.#logger?.log("info", "secrets", "Created a secret", {
      secretId: created.id,
      type: created.type,
    });
    return toSummary(created, now, 0);
  }

  update(id: string, input: SecretUpdateInput): SecretSummary {
    this.#require(id);
    const now = timestampNow(this.#clock);
    const patch: SecretPatch = { updatedAt: now };
    if (input.name !== undefined) patch.name = requireText(input.name, "name");
    if (input.scope !== undefined) patch.scope = input.scope;
    if (input.fields !== undefined) patch.fields = JSON.stringify(input.fields);
    if (input.tags !== undefined) patch.tags = JSON.stringify(input.tags);
    if (input.note !== undefined) patch.note = input.note;
    if (input.rotationDays !== undefined) {
      patch.rotationDays = input.rotationDays;
      patch.rotatesAt = input.rotatesAt ?? rotationDueAt(input.rotationDays, now);
    } else if (input.rotatesAt !== undefined) {
      patch.rotatesAt = input.rotatesAt;
    }
    if (input.value !== undefined) {
      const value = requireValue(input.value);
      patch.cipher = this.#crypto.encrypt(value);
      patch.hint = computeHint(value);
    }
    const updated = this.#data.repositories.secrets.update(id, patch);
    if (updated === undefined) throw notFound(id);
    this.#logger?.log("info", "secrets", "Updated a secret", {
      secretId: id,
      replacedValue: input.value !== undefined,
    });
    return toSummary(updated, now, this.#countUsage(id));
  }

  remove(id: string): void {
    this.#require(id);
    const consumers = this.usage(id);
    if (consumers.length > 0) {
      throw new AppError(
        AppErrorCode.CONFLICT,
        `Секрет используется (${consumers.length}) и не может быть удалён`,
        { details: { secretId: id, consumers } },
      );
    }
    this.#data.repositories.secrets.remove(id);
    this.#logger?.log("info", "secrets", "Removed a secret", { secretId: id });
  }

  usage(id: string): SecretConsumer[] {
    return this.#data.repositories.secrets
      .listUsage(id)
      .map((entry) => ({ kind: entry.consumerKind, id: entry.consumerId }));
  }

  addUsage(secretId: string, kind: SecretConsumerKind, consumerId: string): void {
    this.#require(secretId);
    this.#data.repositories.secrets.addUsage({
      secretId,
      consumerKind: kind,
      consumerId,
      createdAt: timestampNow(this.#clock),
    });
  }

  removeUsage(secretId: string, kind: SecretConsumerKind, consumerId: string): void {
    this.#data.repositories.secrets.removeUsage(secretId, kind, consumerId);
  }

  /**
   * HOST-INTERNAL ONLY — returns the decrypted credential.
   *
   * The result must never be stored, logged, placed in a DTO or forwarded anywhere, and this
   * method must never be reachable from an IPC handler: the renderer sees `hint` and nothing
   * else. Call it at the point of use inside a host service and let the plaintext die with
   * the call.
   */
  async resolve(id: string): Promise<string> {
    const row = this.#data.repositories.secrets.findCipherById(id);
    if (row === undefined) throw notFound(id);
    if (row.cipher === null || row.cipher === undefined || row.cipher.length === 0) {
      throw new AppError(AppErrorCode.SECRET_MISSING, "У секрета нет сохранённого значения", {
        details: { secretId: id },
      });
    }
    return await Promise.resolve(this.#crypto.decrypt(row.cipher));
  }

  #countUsage(id: string): number {
    return this.#data.repositories.secrets.countUsage(id);
  }

  #require(id: string): SecretRow {
    const row = this.#data.repositories.secrets.findById(id);
    if (row === undefined) throw notFound(id);
    return row;
  }
}

function toSummary(row: SecretRow, now: Timestamp, usageCount: number): SecretSummary {
  return {
    id: row.id,
    type: row.type,
    name: row.name,
    scope: row.scope,
    cipherVersion: row.cipherVersion,
    hint: row.hint,
    usageCount,
    fields: decodeFields(row.id, row.fields),
    tags: decodeTags(row.id, row.tags),
    note: row.note,
    rotationDays: row.rotationDays,
    rotatesAt: row.rotatesAt,
    rotation: rotationStatus(row.rotatesAt, now),
    createdAt: row.createdAt,
    updatedAt: row.updatedAt,
  };
}

function decodeFields(id: string, raw: string): Record<string, Json> {
  const parsed = parse(id, "fields", raw);
  return typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)
    ? (parsed as Record<string, Json>)
    : {};
}

function decodeTags(id: string, raw: string): string[] {
  const parsed = parse(id, "tags", raw);
  return Array.isArray(parsed)
    ? parsed.filter((tag): tag is string => typeof tag === "string")
    : [];
}

function parse(id: string, column: string, raw: string): Json {
  try {
    return JSON.parse(raw) as Json;
  } catch (error: unknown) {
    throw new AppError(AppErrorCode.DB_ERROR, "Секрет хранится в повреждённом виде", {
      cause: error,
      details: { secretId: id, column },
    });
  }
}

function requireText(value: string, field: string): string {
  const trimmed = value.trim();
  if (trimmed.length === 0) {
    throw new AppError(AppErrorCode.VALIDATION_FAILED, "Поле не может быть пустым", {
      details: { field },
    });
  }
  return trimmed;
}

function requireValue(value: string): string {
  if (value.trim().length === 0) {
    throw new AppError(AppErrorCode.VALIDATION_FAILED, "Поле не может быть пустым", {
      details: { field: "value" },
    });
  }
  return value;
}

function notFound(id: string): AppError {
  return new AppError(AppErrorCode.NOT_FOUND, "Секрет не найден", { details: { secretId: id } });
}

export function toSecretSummaryDto(summary: SecretSummary): SecretSummaryDto {
  return {
    id: summary.id as SecretId,
    type: summary.type as SecretTypeKey,
    name: summary.name,
    scope: summary.scope,
    hint: summary.hint,
    tags: [...summary.tags],
    usageCount: summary.usageCount,
    rotationStatus: summary.rotation,
    rotatesAt: summary.rotatesAt,
    updatedAt: summary.updatedAt,
  };
}

export function toSecretDto(summary: SecretSummary): SecretDto {
  return {
    ...toSecretSummaryDto(summary),
    fields: { ...summary.fields },
    note: summary.note,
  };
}
