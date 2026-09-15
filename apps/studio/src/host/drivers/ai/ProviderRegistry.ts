import { AppError, AppErrorCode, isLocalFamily } from "@zvs/shared";
import type { AccountEntity, ProviderEntity } from "../../data/schema/index.ts";
import type { Logger } from "../../platform/logger.ts";
import {
  adapterEntry,
  type AdapterContext,
  type LocalEmbeddingEngine,
  type LocalModelStore,
} from "./adapters/index.ts";
import { OllamaAdapter, OLLAMA_CAPABILITIES } from "./adapters/ollama.ts";
import { supportsAuthMode, type AdapterCapabilities } from "./AdapterCapabilities.ts";
import { sessionExpired } from "./errors.ts";
import type { AiDriver, EmbeddingDriver, ImageDriver, TextGenerationDriver } from "./ports.ts";
import { AccountTransport, type AccountCredentials } from "./transport/AccountTransport.ts";
import { ApiTransport, type FetchLike } from "./transport/ApiTransport.ts";
import type { SessionGatewayFactory } from "./transport/SessionGateway.ts";
import type { Transport } from "./transport/Transport.ts";

export interface ProviderSource {
  findById(id: string): ProviderEntity | undefined;
}

export interface SecretResolver {
  resolve(id: string): Promise<string>;
}

export interface AccountSource {
  getById(id: string): AccountEntity | undefined;
}

export type AccountCredentialsFactory = (account: AccountEntity) => AccountCredentials;

export interface ProviderRegistryOptions {
  providers: ProviderSource;
  secrets: SecretResolver;
  accounts?: AccountSource;
  sessions?: SessionGatewayFactory;
  credentials?: AccountCredentialsFactory;
  localModels?: LocalModelStore;
  localEngine?: LocalEmbeddingEngine;
  logger?: Logger;
  fetch?: FetchLike;
}

interface CacheEntry {
  readonly driver: AiDriver;
  readonly fingerprint: string;
  readonly secretId: string | null;
  readonly accountId: string | null;
}

export class ProviderRegistry {
  readonly #providers: ProviderSource;
  readonly #secrets: SecretResolver;
  readonly #accounts: AccountSource | undefined;
  readonly #sessions: SessionGatewayFactory | undefined;
  readonly #credentials: AccountCredentialsFactory | undefined;
  readonly #localModels: LocalModelStore | undefined;
  #localEngine: LocalEmbeddingEngine | undefined;
  readonly #logger: Logger | undefined;
  readonly #fetch: FetchLike | undefined;
  readonly #cache = new Map<string, CacheEntry>();

  constructor(options: ProviderRegistryOptions) {
    this.#providers = options.providers;
    this.#secrets = options.secrets;
    this.#accounts = options.accounts;
    this.#sessions = options.sessions;
    this.#credentials = options.credentials;
    this.#localModels = options.localModels;
    this.#localEngine = options.localEngine;
    this.#logger = options.logger;
    this.#fetch = options.fetch;
  }

  get size(): number {
    return this.#cache.size;
  }

  /**
   * The engine is built after the registry — it needs the download service, which needs the
   * kernel — so it is joined here rather than in the constructor. Cached drivers are dropped so
   * a provider built before the engine existed picks it up.
   */
  attachLocalEngine(engine: LocalEmbeddingEngine): void {
    this.#localEngine = engine;
    this.clear();
  }

  capabilities(providerId: string): AdapterCapabilities {
    const row = this.#row(providerId);
    if (isLocalFamily(row.adapter)) return adapterEntry(row.adapter).capabilities;
    return row.kind === "ollama" ? OLLAMA_CAPABILITIES : adapterEntry(row.adapter).capabilities;
  }

  async driver(providerId: string): Promise<AiDriver> {
    const row = this.#row(providerId);
    const fingerprint = this.#fingerprint(row);
    const cached = this.#cache.get(providerId);
    if (cached !== undefined && cached.fingerprint === fingerprint) return cached.driver;

    const driver = await this.ephemeralDriver(row);
    this.#cache.set(providerId, {
      driver,
      fingerprint,
      secretId: row.secretId,
      accountId: row.accountId,
    });
    return driver;
  }

  async ephemeralDriver(row: ProviderEntity): Promise<AiDriver> {
    if (row.kind === "ollama" && !isLocalFamily(row.adapter)) {
      const transport = await this.#transport(row);
      return { text: new OllamaAdapter(transport, this.#logger), embedding: null, image: null };
    }
    const entry = adapterEntry(row.adapter);
    const local = isLocalFamily(row.adapter);
    if (!local) this.#requireAuthMode(entry.capabilities, row);
    const context: AdapterContext = {
      ...(local ? {} : { transport: await this.#transport(row) }),
      ...(this.#logger === undefined ? {} : { logger: this.#logger }),
      ...(this.#localModels === undefined ? {} : { localModels: this.#localModels }),
      ...(this.#localEngine === undefined ? {} : { localEngine: this.#localEngine }),
    };
    return entry.build(context);
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

  invalidateByAccount(accountId: string): void {
    for (const [providerId, entry] of this.#cache) {
      if (entry.accountId === accountId) this.invalidate(providerId);
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
      case "account": {
        const account = this.#account(row);
        return new AccountTransport({
          baseUrl: row.baseUrl,
          session: this.#session(account),
          ...(this.#credentials === undefined ? {} : { credentials: this.#credentials(account) }),
          timeoutSeconds: row.settings.timeoutSeconds,
          ...(this.#logger === undefined ? {} : { logger: this.#logger }),
        });
      }
      default:
        throw new AppError(AppErrorCode.VALIDATION_FAILED, "Неизвестный режим авторизации", {
          details: { providerId: row.id, authMode: String(row.authMode) },
        });
    }
  }

  #account(row: ProviderEntity): AccountEntity {
    if (row.accountId === null) {
      throw sessionExpired({ providerId: row.id, reason: "account-not-linked" });
    }
    const account = this.#accounts?.getById(row.accountId);
    if (account === undefined) {
      throw sessionExpired({
        providerId: row.id,
        accountId: row.accountId,
        reason: "account-not-linked",
      });
    }
    if (account.status !== "linked") {
      throw sessionExpired({
        providerId: row.id,
        accountId: account.id,
        status: account.status,
        ...(account.statusDetail === null ? {} : { statusDetail: account.statusDetail }),
      });
    }
    return account;
  }

  #session(account: AccountEntity) {
    if (this.#sessions === undefined) {
      throw new AppError(AppErrorCode.VALIDATION_FAILED, "Сессия браузера недоступна", {
        details: { accountId: account.id, partition: account.partition },
      });
    }
    return this.#sessions(account.partition);
  }

  #fingerprint(row: ProviderEntity): string {
    const account = row.accountId === null ? undefined : this.#accounts?.getById(row.accountId);
    return [
      fingerprintOf(row),
      `account=${row.accountId ?? ""}`,
      `accountStatus=${account?.status ?? ""}`,
      `accountUpdated=${account?.updatedAt ?? ""}`,
    ].join("|");
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
