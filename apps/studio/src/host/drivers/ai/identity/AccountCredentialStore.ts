import { AppError, AppErrorCode, type AccountFamily, type AccountIdentity } from "@zvs/shared";
import type { AccountEntity } from "../../../data/schema/index.ts";
import type { AccountRepository } from "../../../data/repositories/index.ts";
import type { Logger } from "../../../platform/logger.ts";
import type { SecretCreateInput, SecretUpdateInput } from "../../../services/SecretService.ts";
import { isSessionExpired, sessionExpired } from "../errors.ts";
import type { AccountCredentials, AccountToken } from "../transport/AccountTransport.ts";
import { DEFAULT_TOKEN_TYPE } from "../transport/AccountTransport.ts";
import type { SessionGateway } from "../transport/SessionGateway.ts";
import { identityProbe } from "./registry.ts";
import type { IdentityProbe, ProbeResult } from "./IdentityProbe.ts";

export const ACCOUNT_TOKEN_SECRET_TYPE = "account-token";
export const TOKEN_SKEW_SECONDS = 60;

export interface AccountSecretVault {
  create(input: SecretCreateInput): { readonly id: string };
  update(id: string, input: SecretUpdateInput): unknown;
  resolve(id: string): Promise<string>;
  remove(id: string): void;
}

export interface AccountCredentialStoreOptions {
  accountId: string;
  accounts: AccountRepository;
  secrets: AccountSecretVault;
  session: SessionGateway;
  probes?: (family: AccountFamily) => IdentityProbe;
  clock?: () => number;
  logger?: Logger;
}

export class AccountCredentialStore implements AccountCredentials {
  readonly #accountId: string;
  readonly #accounts: AccountRepository;
  readonly #secrets: AccountSecretVault;
  readonly #session: SessionGateway;
  readonly #probes: (family: AccountFamily) => IdentityProbe;
  readonly #clock: () => number;
  readonly #logger: Logger | undefined;

  constructor(options: AccountCredentialStoreOptions) {
    this.#accountId = options.accountId;
    this.#accounts = options.accounts;
    this.#secrets = options.secrets;
    this.#session = options.session;
    this.#probes = options.probes ?? identityProbe;
    this.#clock = options.clock ?? Date.now;
    this.#logger = options.logger;
  }

  get accountId(): string {
    return this.#accountId;
  }

  async current(signal: AbortSignal): Promise<AccountToken | null> {
    const row = this.#row();
    if (row.status !== "linked") {
      throw sessionExpired({ accountId: row.id, status: row.status });
    }
    if (row.tokenSecretId === null) return null;
    if (this.#stale(row)) return await this.refresh(signal);
    return { token: await this.#secrets.resolve(row.tokenSecretId), tokenType: DEFAULT_TOKEN_TYPE };
  }

  async refresh(signal: AbortSignal): Promise<AccountToken | null> {
    const row = this.#row();
    const probe = this.#probes(row.adapter);
    let result: ProbeResult;
    try {
      result = await probe.probe(this.#session, signal);
    } catch (error: unknown) {
      if (isSessionExpired(error)) this.#markNeedsRelink(row, describe(error));
      throw error;
    }
    if (result.identity.externalId !== row.externalId) {
      this.#markNeedsRelink(row, "В браузере выполнен вход под другим аккаунтом");
      throw sessionExpired({ accountId: row.id, reason: "identity-mismatch" });
    }
    return this.#store(row, result);
  }

  async probe(signal: AbortSignal): Promise<AccountIdentity> {
    await this.refresh(signal);
    return toIdentity(this.#row());
  }

  unlink(): void {
    const removal = this.#accounts.remove(this.#accountId, this.#clock());
    if (removal.freedSecretId !== null) this.#secrets.remove(removal.freedSecretId);
    this.#logger?.log("info", "ai", "Unlinked an account", {
      accountId: this.#accountId,
      detachedProviders: removal.detachedProviderIds.length,
      deletedToken: removal.freedSecretId !== null,
    });
  }

  #store(row: AccountEntity, result: ProbeResult): AccountToken | null {
    const now = this.#clock();
    const { identity, credential } = result;
    const tokenSecretId =
      credential === null ? row.tokenSecretId : this.#writeToken(row, credential.token);
    this.#accounts.update(row.id, {
      emailMasked: identity.emailMasked ?? null,
      displayName: identity.displayName ?? null,
      avatarUrl: identity.avatarUrl ?? null,
      tokenSecretId,
      tokenExpiresAt: identity.expiresAt ?? null,
      status: "linked",
      statusDetail: null,
      lastCheckedAt: now,
      updatedAt: now,
    });
    this.#logger?.log("info", "ai", "Refreshed an account identity", {
      accountId: row.id,
      adapter: row.adapter,
      hasToken: credential !== null,
      expiresAt: identity.expiresAt ?? null,
    });
    if (credential === null) return null;
    return { token: credential.token, tokenType: credential.tokenType };
  }

  #writeToken(row: AccountEntity, token: string): string {
    if (row.tokenSecretId !== null) {
      this.#secrets.update(row.tokenSecretId, { value: token });
      return row.tokenSecretId;
    }
    return this.#secrets.create({
      type: ACCOUNT_TOKEN_SECRET_TYPE,
      name: `${row.adapter}:${row.externalId}`,
      scope: "personal",
      value: token,
    }).id;
  }

  #markNeedsRelink(row: AccountEntity, detail: string): void {
    this.#accounts.updateStatus(row.id, "needs-relink", detail, this.#clock());
    this.#logger?.log("warn", "ai", "Account needs a re-link", {
      raw: detail,
      accountId: row.id,
      adapter: row.adapter,
    });
  }

  #stale(row: AccountEntity): boolean {
    if (row.tokenExpiresAt === null) return false;
    return row.tokenExpiresAt - TOKEN_SKEW_SECONDS <= Math.floor(this.#clock() / 1000);
  }

  #row(): AccountEntity {
    const row = this.#accounts.getById(this.#accountId);
    if (row === undefined) {
      throw new AppError(AppErrorCode.NOT_FOUND, "Аккаунт не найден", {
        details: { accountId: this.#accountId },
      });
    }
    return row;
  }
}

export function toIdentity(row: AccountEntity): AccountIdentity {
  return {
    externalId: row.externalId,
    ...(row.emailMasked === null ? {} : { emailMasked: row.emailMasked }),
    ...(row.displayName === null ? {} : { displayName: row.displayName }),
    ...(row.avatarUrl === null ? {} : { avatarUrl: row.avatarUrl }),
    ...(row.tokenExpiresAt === null ? {} : { expiresAt: row.tokenExpiresAt }),
  };
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : "Сессия недействительна";
}
