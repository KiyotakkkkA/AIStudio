import { AppError, AppErrorCode, timestampNow, type Json, type Timestamp } from "@zvs/shared";
import type { UnitOfWork } from "../data/UnitOfWork.ts";

export interface SettingDto {
  key: string;
  value: Json;
  updatedAt: Timestamp;
}

export interface SettingServiceOptions {
  data: UnitOfWork;
  clock?: () => number;
}

export class SettingService {
  readonly #data: UnitOfWork;
  readonly #clock: () => number;

  constructor(options: SettingServiceOptions) {
    this.#data = options.data;
    this.#clock = options.clock ?? Date.now;
  }

  get(key: string): SettingDto | undefined {
    const entity = this.#data.repositories.settings.get(key);
    return entity === undefined ? undefined : toDto(entity);
  }

  all(): SettingDto[] {
    return this.#data.repositories.settings.all().map(toDto);
  }

  set(key: string, value: unknown): SettingDto {
    const encoded = encode(key, value);
    return toDto(this.#data.repositories.settings.set(key, encoded, timestampNow(this.#clock)));
  }

  setMany(entries: Readonly<Record<string, unknown>>): SettingDto[] {
    const encoded = Object.entries(entries).map(
      ([key, value]) => [key, encode(key, value)] as const,
    );
    const updatedAt = timestampNow(this.#clock);
    return this.#data.transaction((repositories) =>
      encoded.map(([key, value]) => toDto(repositories.settings.set(key, value, updatedAt))),
    );
  }

  remove(key: string): void {
    this.#data.repositories.settings.remove(key);
  }
}

function encode(key: string, value: unknown): string {
  const encoded = JSON.stringify(value);
  if (encoded === undefined) {
    throw new AppError(AppErrorCode.VALIDATION_FAILED, "Значение настройки не сериализуется", {
      details: { key },
    });
  }
  return encoded;
}

function toDto(entity: { key: string; value: string; updatedAt: number }): SettingDto {
  try {
    return {
      key: entity.key,
      value: JSON.parse(entity.value) as Json,
      updatedAt: entity.updatedAt as Timestamp,
    };
  } catch (error: unknown) {
    throw new AppError(AppErrorCode.DB_ERROR, "Настройка хранится в повреждённом виде", {
      cause: error,
      details: { key: entity.key },
    });
  }
}
