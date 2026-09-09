import { AppError, AppErrorCode } from "@zvs/shared";
import type { ProviderEntity } from "../../data/schema/index.ts";
import type { Logger } from "../../platform/logger.ts";
import { adapterEntry, type AdapterContext } from "./adapters/index.ts";
import { supportsAuthMode, type AdapterCapabilities } from "./AdapterCapabilities.ts";
import type { AiDriver, EmbeddingDriver, ImageDriver, TextGenerationDriver } from "./ports.ts";
import { ApiTransport, type FetchLike } from "./transport/ApiTransport.ts";
import type { Transport } from "./transport/Transport.ts";

export interface ProviderSource {
  findById(id: string): ProviderEntity | undefined;
}

export interface SecretResolver {
  resolve(id: string): Promise<string>;
}

export interface ProviderRegistryOptions {
  providers: ProviderSource;
  secrets: SecretResolver;
  logger?: Logger;
  fetch?: FetchLike;
}

interface CacheEntry {
  readonly driver: AiDriver;
  readonly fingerprint: string;
  readonly secretId: string | null;
}

export class ProviderRegistry {
  readonly #providers: ProviderSource;
  readonly #secrets: SecretResolver;
  readonly #logger: Logger | undefined;
  readonly #fetch: FetchLike | undefined;
  readonly #cache = new Map<string, CacheEntry>();

  constructor(options: ProviderRegistryOptions) {
    this.#providers = options.providers;
    this.#secrets = options.secrets;
    this.#logger = options.logger;
    this.#fetch = options.fetch;
  }

  get size(): number {
    return this.#cache.size;
  }

  capabilities(providerId: string): AdapterCapabilities {
    return adapterEntry(this.#row(providerId).adapter).capabilities;
  }

  async driver(providerId: string): Promise<AiDriver> {
    const row = this.#row(providerId);
    const fingerprint = fingerprintOf(row);
    const cached = this.#cache.get(providerId);
    if (cached !== undefined && cached.fingerprint === fingerprint) return cached.driver;

    const entry = adapterEntry(row.adapter);
    this.#requireAuthMode(entry.capabilities, row);
    const transport = await this.#transport(row);
    const context: AdapterContext = {
      transport,
      ...(this.#logger === undefined ? {} : { logger: this.#logger }),
    };
    const driver = entry.build(context);
    this.#cache.set(providerId, { driver, fingerprint, secretId: row.secretId });
    return driver;
  }

  async text(providerId: string): Promise<TextGenerationDriver> {
    const driver = await this.driver(providerId);
    if (driver.text === null) throw unsupported(providerId, "text");
    return driver.text;
  }

  async embedding(providerId: string): Promise<EmbeddingDriver> {
    const driver = await this.driver(providerId);
    if (driver.embedding === null) throw unsupported(providerId, "embedding");
    return driver.embedding;
  }

  async image(providerId: string): Promise<ImageDriver> {
    const driver = await this.driver(providerId);
    if (driver.image === null) throw unsupported(providerId, "image");
    return driver.image;
  }

  invalidate(providerId: string): void {
    if (this.#cache.delete(providerId)) {
      this.#logger?.log("debug", "ai", "Dropped a cached provider driver", { providerId });
    }
  }

  invalidateBySecret(secretId: string): void {
    for (const [providerId, entry] of this.#cache) {
      if (entry.secretId === secretId) this.invalidate(providerId);
    }
  }

  clear(): void {
    this.#cache.clear();
  }

  async #transport(row: ProviderEntity): Promise<Transport> {
    switch (row.authMode) {
      case "api":
        return new ApiTransport({
          baseUrl: row.baseUrl,
          apiKey: row.secretId === null ? undefined : await this.#secrets.resolve(row.secretId),
          timeoutSeconds: row.settings.timeoutSeconds,
          ...(this.#logger === undefined ? {} : { logger: this.#logger }),
          ...(this.#fetch === undefined ? {} : { fetch: this.#fetch }),
        });
      case "account":
        throw new AppError(AppErrorCode.UNKNOWN, "auth mode account is not implemented yet", {
          details: { providerId: row.id, authMode: row.authMode },
        });
      default:
        throw new AppError(AppErrorCode.VALIDATION_FAILED, "Неизвестный режим авторизации", {
          details: { providerId: row.id, authMode: String(row.authMode) },
        });
    }
  }

  #requireAuthMode(capabilities: AdapterCapabilities, row: ProviderEntity): void {
    if (supportsAuthMode(capabilities, row.authMode)) return;
    throw new AppError(
      AppErrorCode.VALIDATION_FAILED,
      `adapter ${capabilities.family} cannot run in ${row.authMode} mode`,
      { details: { providerId: row.id, family: capabilities.family, authMode: row.authMode } },
    );
  }

  #row(providerId: string): ProviderEntity {
    const row = this.#providers.findById(providerId);
    if (row === undefined) {
      throw new AppError(AppErrorCode.NOT_FOUND, "Провайдер не найден", {
        details: { providerId },
      });
    }
    if (!row.enabled) {
      throw new AppError(AppErrorCode.CONFLICT, "Провайдер отключён", { details: { providerId } });
    }
    return row;
  }
}

function fingerprintOf(row: ProviderEntity): string {
  return [
    `adapter=${row.adapter}`,
    `auth=${row.authMode}`,
    `base=${row.baseUrl}`,
    `secret=${row.secretId ?? ""}`,
    `timeout=${row.settings.timeoutSeconds ?? ""}`,
    `updated=${row.updatedAt}`,
  ].join("|");
}

function unsupported(providerId: string, capability: string): AppError {
  return new AppError(AppErrorCode.CONFLICT, "Провайдер не поддерживает эту возможность", {
    details: { providerId, capability },
  });
}
