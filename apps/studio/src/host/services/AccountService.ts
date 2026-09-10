import {
  ACCOUNT_FAMILIES,
  AccountDto,
  AccountFamily,
  AppError,
  AppErrorCode,
  type AccountLinkResult,
} from "@zvs/shared";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import type { AccountEntity } from "../data/schema/account.ts";
import { BROWSER_PARTITION } from "../browser/policy.ts";
import type { BrowserLifecycle, BrowserLifecycleEvent } from "../browser/lifecycle.ts";
import { identityProbe } from "../drivers/ai/identity/registry.ts";
import type { IdentityProbe, ProbeResult } from "../drivers/ai/identity/IdentityProbe.ts";
import { ACCOUNT_TOKEN_SECRET_TYPE } from "../drivers/ai/identity/AccountCredentialStore.ts";
import type { SessionGatewayFactory } from "../drivers/ai/transport/SessionGateway.ts";
import { isSessionExpired } from "../drivers/ai/errors.ts";
import type { EventBus } from "../platform/events.ts";
import type { Logger } from "../platform/logger.ts";
import type { SecretService } from "./SecretService.ts";

export interface AccountBrowserOpener {
  openTab(url: string): void;
}

export interface AccountServiceOptions {
  data: UnitOfWork;
  secrets: SecretService;
  probes?: (family: AccountFamily) => IdentityProbe;
  sessions: SessionGatewayFactory;
  browser: AccountBrowserOpener;
  lifecycle: BrowserLifecycle;
  events: EventBus;
  clock?: () => number;
  random?: () => number;
  logger?: Logger;
  timeoutMs?: number;
  intervalMs?: number;
  onLinked?: () => void;
}

export class AccountService {
  readonly #options: AccountServiceOptions;
  readonly #links = new Map<
    AccountFamily,
    { controller: AbortController; result: Promise<AccountLinkResult> }
  >();
  readonly #refreshes = new Map<
    string,
    { controller: AbortController; result: Promise<AccountDto> }
  >();
  readonly #unsubscribe: () => void;
  #disposed = false;

  constructor(options: AccountServiceOptions) {
    this.#options = options;
    this.#unsubscribe = options.lifecycle.subscribe((event) => this.#browserEvent(event));
  }

  list(): AccountDto[] {
    return ACCOUNT_FAMILIES.flatMap((adapter) =>
      this.#options.data.repositories.accounts.listByAdapter(adapter),
    ).map((row) => this.#dto(row));
  }

  link(input: { adapter: AccountFamily }): Promise<AccountLinkResult> {
    this.#requireActive();
    const adapter = AccountFamily.parse(input.adapter);
    const existing = this.#links.get(adapter);
    if (existing) return existing.result;
    for (const row of this.#options.data.repositories.accounts.listByAdapter(adapter)) {
      this.#refreshes.get(row.id)?.controller.abort();
    }
    const controller = new AbortController();
    const result = Promise.resolve()
      .then(() => this.#poll(adapter, controller))
      .finally(() => this.#links.delete(adapter));
    this.#links.set(adapter, { controller, result });
    return result;
  }

  cancelLink(input: { adapter: AccountFamily }): { cancelled: boolean } {
    const pending = this.#links.get(input.adapter);
    pending?.controller.abort();
    return { cancelled: pending !== undefined };
  }

  unlink(id: string): { id: AccountDto["id"]; removed: true; browserSessionPreserved: true } {
    const row = this.#require(id);
    this.cancelLink({ adapter: row.adapter });
    this.#refreshes.get(id)?.controller.abort();
    this.#options.data.transaction((repositories) => {
      const removal = repositories.accounts.remove(id, this.#now());
      if (removal.freedSecretId !== null) this.#options.secrets.remove(removal.freedSecretId);
    });
    return { id: AccountDto.shape.id.parse(id), removed: true, browserSessionPreserved: true };
  }

  refresh(id: string): Promise<AccountDto> {
    this.#requireActive();
    const row = this.#require(id);
    if (this.#links.has(row.adapter))
      throw new AppError(AppErrorCode.CONFLICT, "Account sign-in is in progress");
    const pending = this.#refreshes.get(id);
    if (pending) return pending.result;
    const controller = new AbortController();
    const result = this.#refresh(row, controller).finally(() => this.#refreshes.delete(id));
    this.#refreshes.set(id, { controller, result });
    return result;
  }

  async dispose(): Promise<void> {
    this.#disposed = true;
    this.#unsubscribe();
    const pending = [...this.#links.values(), ...this.#refreshes.values()];
    for (const operation of pending) operation.controller.abort();
    await Promise.allSettled(pending.map((operation) => operation.result));
  }

  async #poll(adapter: AccountFamily, controller: AbortController): Promise<AccountLinkResult> {
    const { signal } = controller;
    const stream = this.#options.events.openStream();
    const timeoutMs = this.#options.timeoutMs ?? 180_000;
    let timeout = false;
    const timer = setTimeout(() => {
      timeout = true;
      controller.abort();
    }, timeoutMs);
    const start = this.#now();
    try {
      signal.throwIfAborted();
      const probe = this.#probe(adapter);
      this.#options.browser.openTab(probe.loginUrl);
      while (true) {
        signal.throwIfAborted();
        stream.emit({
          type: "step",
          step: { domain: "accounts", adapter, phase: "waiting-for-sign-in" },
        });
        stream.emit({
          type: "progress",
          done: Math.min(timeoutMs, Math.max(0, this.#now() - start)),
          total: timeoutMs,
        });
        let result: ProbeResult;
        try {
          result = await abortable(
            probe.probe(this.#options.sessions(BROWSER_PARTITION), signal),
            signal,
          );
        } catch (error: unknown) {
          signal.throwIfAborted();
          if (!isSessionExpired(error))
            throw new AppError(
              AppErrorCode.PROVIDER_UNREACHABLE,
              "Could not check the vendor session",
            );
          await delay(
            (this.#options.intervalMs ?? 2000) *
              (0.9 + (this.#options.random ?? Math.random)() * 0.2),
            signal,
          );
          continue;
        }
        signal.throwIfAborted();
        const linked = this.#save(adapter, result);
        stream.emit({
          type: "step",
          step: { domain: "accounts", adapter, phase: "done", result: linked },
        });
        stream.end({ status: "ok" });
        this.#options.onLinked?.();
        return linked;
      }
    } catch (error: unknown) {
      if (signal.aborted) {
        const result: AccountLinkResult = {
          status: timeout ? "timeout" : "cancelled",
          detail: "No sign-in detected",
        };
        stream.emit({ type: "step", step: { domain: "accounts", adapter, phase: "done", result } });
        stream.end({ status: "cancelled" });
        return result;
      }
      stream.end({ status: "failed", message: "Account linking failed" });
      this.#options.logger?.log("warn", "accounts", "Account linking failed", { adapter });
      throw error;
    } finally {
      clearTimeout(timer);
      this.#options.lifecycle.emit({ type: "link-finished", url: this.#probe(adapter).loginUrl });
    }
  }

  async #refresh(row: AccountEntity, controller: AbortController): Promise<AccountDto> {
    const timer = setTimeout(() => controller.abort(), this.#options.timeoutMs ?? 180_000);
    const { signal } = controller;
    try {
      const result = await abortable(
        this.#probe(row.adapter).probe(this.#options.sessions(row.partition), signal),
        signal,
      );
      signal.throwIfAborted();
      if (result.identity.externalId !== row.externalId) {
        this.#options.data.repositories.accounts.updateStatus(
          row.id,
          "needs-relink",
          "Browser is signed in to a different account",
          this.#now(),
        );
      } else {
        this.#save(row.adapter, result);
      }
    } catch (error: unknown) {
      signal.throwIfAborted();
      this.#options.data.repositories.accounts.update(row.id, {
        lastCheckedAt: this.#now(),
        updatedAt: this.#now(),
      });
      if (!isSessionExpired(error))
        throw new AppError(AppErrorCode.PROVIDER_UNREACHABLE, "Could not check the vendor session");
      this.#options.data.repositories.accounts.updateStatus(
        row.id,
        "needs-relink",
        "Session expired — sign in again",
        this.#now(),
      );
    } finally {
      clearTimeout(timer);
    }
    return this.#dto(this.#require(row.id));
  }

  #save(
    adapter: AccountFamily,
    result: ProbeResult,
  ): Extract<AccountLinkResult, { status: "linked" }> {
    const account = this.#options.data.transaction((repositories) => {
      const previous = repositories.accounts.listByAdapter(adapter);
      const matching = repositories.accounts.findByAdapterAndExternalId(
        adapter,
        result.identity.externalId,
      );
      const replaced = previous.some((row) => row.externalId !== result.identity.externalId);
      for (const row of previous) {
        if (row.id === matching?.id) continue;
        const removal = repositories.accounts.remove(row.id, this.#now());
        if (removal.freedSecretId !== null) this.#options.secrets.remove(removal.freedSecretId);
      }
      const now = this.#now();
      let tokenSecretId = matching?.tokenSecretId ?? null;
      if (result.credential !== null) {
        if (tokenSecretId === null) {
          tokenSecretId = this.#options.secrets.create({
            type: ACCOUNT_TOKEN_SECRET_TYPE,
            name: `${adapter} account token`,
            scope: "personal",
            value: result.credential.token,
          }).id;
        } else this.#options.secrets.update(tokenSecretId, { value: result.credential.token });
      } else tokenSecretId = null;
      const fields = {
        externalId: result.identity.externalId,
        emailMasked: maskEmail(result.identity.emailMasked),
        displayName: result.identity.displayName ?? null,
        avatarUrl: result.identity.avatarUrl ?? null,
        tokenSecretId,
        tokenExpiresAt: result.identity.expiresAt ?? null,
        status: "linked" as const,
        statusDetail: null,
        lastCheckedAt: now,
        updatedAt: now,
      };
      const row = matching
        ? repositories.accounts.update(matching.id, fields)!
        : repositories.accounts.create({
            ...fields,
            adapter,
            partition: BROWSER_PARTITION,
            linkedAt: now,
          });
      if (matching?.tokenSecretId && tokenSecretId === null)
        this.#options.secrets.remove(matching.tokenSecretId);
      return { row, replaced };
    });
    return {
      status: "linked",
      account: this.#dto(account.row),
      replacedPreviousIdentity: account.replaced,
      detail: account.replaced
        ? "Previous identity replaced; only one browser session per vendor can be active. Previous providers need an account linked."
        : null,
    };
  }

  #browserEvent(event: BrowserLifecycleEvent): void {
    if (event.type === "workspace-closed") {
      for (const pending of this.#links.values()) pending.controller.abort();
    } else if (event.type === "link-tab-closed") {
      for (const adapter of ACCOUNT_FAMILIES) {
        if (this.#probe(adapter).loginUrl === event.url) this.cancelLink({ adapter });
      }
    } else if (event.type === "cookies-cleared") {
      const domain = event.domain.replace(/^\./, "").toLowerCase();
      for (const adapter of ACCOUNT_FAMILIES) {
        const host = new URL(this.#probe(adapter).loginUrl).hostname;
        if (domain !== host && !host.endsWith(`.${domain}`)) continue;
        this.cancelLink({ adapter });
        for (const row of this.#options.data.repositories.accounts.listByAdapter(adapter)) {
          this.#refreshes.get(row.id)?.controller.abort();
          this.#options.data.repositories.accounts.updateStatus(
            row.id,
            "needs-relink",
            "Browser cookies cleared — sign in again",
            this.#now(),
          );
        }
      }
    }
  }

  #dto(row: AccountEntity): AccountDto {
    return AccountDto.parse({
      id: row.id,
      adapter: row.adapter,
      emailMasked: maskEmail(row.emailMasked),
      displayName: row.displayName,
      avatarUrl: row.avatarUrl,
      status: row.status,
      detail: row.statusDetail,
      expiresAt: row.tokenExpiresAt,
      lastCheckedAt: row.lastCheckedAt,
      linkedProvidersCount: this.#options.data.repositories.providers
        .list()
        .filter((provider) => provider.accountId === row.id).length,
    });
  }

  #require(id: string): AccountEntity {
    const row = this.#options.data.repositories.accounts.getById(id);
    if (!row) throw new AppError(AppErrorCode.NOT_FOUND, "Account not found");
    return row;
  }

  #requireActive(): void {
    if (this.#disposed) throw new AppError(AppErrorCode.CONFLICT, "Account service is closed");
  }

  #probe(adapter: AccountFamily): IdentityProbe {
    return (this.#options.probes ?? identityProbe)(adapter);
  }
  #now(): number {
    return (this.#options.clock ?? Date.now)();
  }
}

function maskEmail(value: string | null | undefined): string | null {
  if (!value) return null;
  const at = value.lastIndexOf("@");
  if (at < 1) return "***";
  return `${value[0]}***${value.slice(at)}`;
}

async function abortable<T>(work: Promise<T>, signal: AbortSignal): Promise<T> {
  let abort: () => void = () => undefined;
  const aborted = new Promise<never>((_, reject) => {
    abort = () => reject(new AppError(AppErrorCode.CONFLICT, "Account operation cancelled"));
    signal.addEventListener("abort", abort, { once: true });
    if (signal.aborted) abort();
  });
  try {
    return await Promise.race([work, aborted]);
  } finally {
    signal.removeEventListener("abort", abort);
  }
}

async function delay(ms: number, signal: AbortSignal): Promise<void> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    await abortable(
      new Promise<void>((resolve) => {
        timer = setTimeout(resolve, ms);
      }),
      signal,
    );
  } finally {
    clearTimeout(timer);
  }
}
