import {
  AppError,
  AppErrorCode,
  timestampNow,
  type AccountId,
  type CreateProviderInput,
  type ModelDto,
  type ModelId,
  type ProbeResultDto,
  type ProviderDto,
  type ProviderId,
  type ProviderListFilter,
  type ProviderSummaryDto,
  type SecretId,
  type Timestamp,
  type UpdateProviderInput,
} from "@zvs/shared";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import type { ProviderPatch, Repositories } from "../data/repositories/index.ts";
import type {
  AccountEntity,
  ModelEntity,
  ProviderEntity,
  ProviderSettings,
} from "../data/schema/index.ts";
import { adapterCapabilities } from "../drivers/ai/adapters/index.ts";
import type { AdapterCapabilities } from "../drivers/ai/AdapterCapabilities.ts";
import { cancelled, rawErrorText, timedOut, toAppError } from "../drivers/ai/errors.ts";
import type { AiDriver, DiscoveredModel } from "../drivers/ai/ports.ts";
import type { Logger } from "../platform/logger.ts";
import { createId } from "../platform/ids.ts";
import type { SecretService } from "./SecretService.ts";
import { discoveryOutcome, failureOutcome, succeeded, type ProbeOutcome } from "./probeOutcome.ts";
import { deriveStatus, type CredentialLifetime, type StatusDerivation } from "./providerStatus.ts";

export const DEFAULT_PROBE_TIMEOUT_SECONDS = 30;

export interface DriverSource {
  ephemeralDriver(row: ProviderEntity): Promise<AiDriver>;
  invalidate(providerId: string): void;
}

export type ProviderDraftInput = Omit<CreateProviderInput, "name" | "enabled"> &
  Partial<Pick<CreateProviderInput, "name" | "enabled">>;

export type ProbeInput = { readonly id: string } | { readonly draft: ProviderDraftInput };

export interface ProbeResult {
  readonly providerId: string | null;
  readonly outcome: ProbeOutcome;
  readonly derived: StatusDerivation;
  readonly checkedAt: Timestamp;
  readonly provider: ProviderDto | null;
}

export interface ProviderServiceOptions {
  data: UnitOfWork;
  drivers: DriverSource;
  secrets: SecretService;
  logger?: Logger;
  clock?: () => number;
  probeTimeoutSeconds?: number;
}

export class ProviderService {
  readonly #data: UnitOfWork;
  readonly #drivers: DriverSource;
  readonly #secrets: SecretService;
  readonly #logger: Logger | undefined;
  readonly #clock: () => number;
  readonly #timeoutSeconds: number;

  readonly #inFlight = new Map<string, Promise<ProbeResult>>();
  readonly #controllers = new Set<AbortController>();
  #disposed = false;

  constructor(options: ProviderServiceOptions) {
    this.#data = options.data;
    this.#drivers = options.drivers;
    this.#secrets = options.secrets;
    this.#logger = options.logger;
    this.#clock = options.clock ?? Date.now;
    this.#timeoutSeconds = options.probeTimeoutSeconds ?? DEFAULT_PROBE_TIMEOUT_SECONDS;
  }

  list(filter: ProviderListFilter = {}): ProviderSummaryDto[] {
    return this.#repositories.providers
      .list(filter)
      .map((row) => toSummaryDto(this.#accountStatus(row), this.#countModels(row.id)));
  }

  get(id: string, selectedOnly = false): ProviderDto {
    return this.#toDto(this.#require(id), selectedOnly);
  }

  create(input: CreateProviderInput): ProviderDto {
    const now = timestampNow(this.#clock);
    const row = this.#repositories.providers.create({
      ...connectionColumns(input),
      name: input.name.trim(),
      enabled: input.enabled,
      createdAt: now,
      updatedAt: now,
    });
    this.#logger?.log("info", "providers", "Created a provider", {
      providerId: row.id,
      adapter: row.adapter,
    });
    return this.#toDto(row);
  }

  update(input: UpdateProviderInput): ProviderDto {
    const { id, ...rest } = input;
    this.#requireIdle(id);
    this.#require(id);
    const patch: ProviderPatch = { updatedAt: timestampNow(this.#clock) };
    if (rest.name !== undefined) patch.name = rest.name.trim();
    if (rest.adapter !== undefined) patch.adapter = rest.adapter;
    if (rest.authMode !== undefined) patch.authMode = rest.authMode;
    if (rest.baseUrl !== undefined) patch.baseUrl = rest.baseUrl;
    if (rest.secretId !== undefined) patch.secretId = rest.secretId;
    if (rest.accountId !== undefined) patch.accountId = rest.accountId;
    if (rest.capabilities !== undefined) patch.capabilities = [...rest.capabilities];
    if (rest.settings !== undefined) patch.settings = { ...rest.settings };
    if (rest.enabled !== undefined) patch.enabled = rest.enabled;
    const row = this.#repositories.providers.update(id, patch);
    if (row === undefined) throw notFound(id);
    this.#drivers.invalidate(id);
    return this.#toDto(row);
  }

  remove(id: string): void {
    this.#requireIdle(id);
    this.#require(id);
    this.#repositories.providers.remove(id);
    this.#drivers.invalidate(id);
    this.#logger?.log("info", "providers", "Removed a provider", { providerId: id });
  }

  setDefaultModel(id: string, modelId: string | null): ProviderDto {
    const row = this.#repositories.providers.setDefaultModel(
      id,
      modelId,
      timestampNow(this.#clock),
    );
    if (row === undefined) throw notFound(id);
    return this.#toDto(row);
  }

  models(id: string): ModelDto[] {
    this.#require(id);
    return this.#repositories.models.listByProvider(id).map(toModelDto);
  }

  isProbing(id: string): boolean {
    return this.#inFlight.has(id);
  }

  probe(input: ProbeInput, signal?: AbortSignal): Promise<ProbeResult> {
    if (this.#disposed) return Promise.reject(cancelled());
    if ("draft" in input) return this.#probeDraft(input.draft);
    const id = input.id;
    const joined = this.#inFlight.get(id);
    if (joined !== undefined) return joined;
    const running = this.#probeSaved(id, signal).finally(() => this.#inFlight.delete(id));
    this.#inFlight.set(id, running);
    return running;
  }

  async refreshAll(options: { onlyIdle?: boolean } = {}): Promise<ProviderSummaryDto[]> {
    for (const row of this.#repositories.providers.list({ enabled: true })) {
      if (options.onlyIdle === true && this.isProbing(row.id)) continue;
      await this.probe({ id: row.id });
    }
    return this.list({ enabled: true });
  }

  async dispose(): Promise<void> {
    this.#disposed = true;
    for (const controller of this.#controllers) controller.abort(cancelled());
    await Promise.allSettled(this.#inFlight.values());
  }

  #requireIdle(id: string): void {
    if (this.isProbing(id)) {
      throw new AppError(AppErrorCode.CONFLICT, "Дождитесь завершения проверки провайдера");
    }
  }

  async #probeSaved(id: string, signal?: AbortSignal): Promise<ProbeResult> {
    const row = this.#require(id);
    const outcome = await this.#runProbe(row, signal);
    if (this.#disposed) throw cancelled();
    signal?.throwIfAborted();
    const checkedAt = timestampNow(this.#clock);
    const derived = deriveStatus({ outcome, lifetimes: this.#lifetimes(row), now: checkedAt });
    const stored = this.#persist(row, outcome, derived, checkedAt);
    return { providerId: id, outcome, derived, checkedAt, provider: this.#toDto(stored) };
  }

  async #probeDraft(draft: ProviderDraftInput): Promise<ProbeResult> {
    const row = draftRow(draft, timestampNow(this.#clock));
    const outcome = await this.#runProbe(row);
    if (this.#disposed) throw cancelled();
    const checkedAt = timestampNow(this.#clock);
    return {
      providerId: null,
      outcome,
      derived: deriveStatus({ outcome, lifetimes: this.#lifetimes(row), now: checkedAt }),
      checkedAt,
      provider: null,
    };
  }

  async #runProbe(row: ProviderEntity, signal?: AbortSignal): Promise<ProbeOutcome> {
    if (row.accountId !== null && this.#account(row.accountId)?.status !== "linked") {
      return { kind: "session-expired" };
    }
    const seconds = row.settings.timeoutSeconds ?? this.#timeoutSeconds;
    const controller = new AbortController();
    const abort = () => controller.abort(cancelled());
    signal?.addEventListener("abort", abort, { once: true });
    if (signal?.aborted === true) abort();
    this.#controllers.add(controller);
    const timer = setTimeout(() => controller.abort(timedOut(seconds)), seconds * 1000);
    const startedAt = this.#clock();
    let onAbort: () => void = () => undefined;
    try {
      const aborted = new Promise<never>((_, reject) => {
        onAbort = () => reject(controller.signal.reason);
        controller.signal.addEventListener("abort", onAbort, { once: true });
        if (controller.signal.aborted) onAbort();
      });
      const discover = async () => {
        const driver = await this.#drivers.ephemeralDriver(row);
        controller.signal.throwIfAborted();
        return discoveryPort(driver, row.id).listModels(controller.signal);
      };
      const models = await Promise.race([discover(), aborted]);
      const latency = Math.max(0, Math.round(this.#clock() - startedAt));
      return discoveryOutcome(latency, models, adapterCapabilities(row.adapter).liveModelList);
    } catch (error: unknown) {
      const outcome = failureOutcome(toAppError(error));
      this.#logger?.log("warn", "providers", "Probe did not succeed", {
        raw: rawErrorText(error),
        providerId: row.id,
        kind: outcome.kind,
      });
      return outcome;
    } finally {
      clearTimeout(timer);
      controller.signal.removeEventListener("abort", onAbort);
      signal?.removeEventListener("abort", abort);
      this.#controllers.delete(controller);
    }
  }

  #persist(
    row: ProviderEntity,
    outcome: ProbeOutcome,
    derived: StatusDerivation,
    checkedAt: Timestamp,
  ): ProviderEntity {
    return this.#data.transaction((repositories) => {
      if (succeeded(outcome)) {
        repositories.models.replaceForProvider(row.id, discoveredRows(outcome, checkedAt));
        const kept = repositories.providers.findById(row.id)?.defaultModelId ?? null;
        if (row.defaultModelId !== null && kept === null) {
          this.#logger?.log("info", "providers", "Default model vanished from the vendor", {
            providerId: row.id,
            defaultModelId: row.defaultModelId,
          });
        }
      }
      const updated = repositories.providers.setStatus(
        row.id,
        derived.status,
        derived.detail,
        latencyOf(outcome),
        checkedAt,
      );
      if (updated === undefined) throw notFound(row.id);
      return updated;
    });
  }

  #lifetimes(row: ProviderEntity): CredentialLifetime[] {
    const lifetimes: CredentialLifetime[] = [];
    if (row.secretId !== null) {
      lifetimes.push({ source: "secret", expiresAt: this.#secretExpiry(row.secretId) });
    }
    if (row.accountId !== null) {
      const account = this.#account(row.accountId);
      lifetimes.push({
        source: "account",
        expiresAt:
          account?.tokenExpiresAt == null ? null : ((account.tokenExpiresAt * 1000) as Timestamp),
      });
    }
    return lifetimes;
  }

  #secretExpiry(secretId: string): Timestamp | null {
    return this.#secretSummary(secretId)?.rotatesAt ?? null;
  }

  #secretSummary(secretId: string) {
    try {
      return this.#secrets.get(secretId);
    } catch {
      return undefined;
    }
  }

  #account(accountId: string): AccountEntity | undefined {
    return this.#repositories.accounts.getById(accountId);
  }

  #toDto(row: ProviderEntity, selectedOnly = false): ProviderDto {
    const models = this.#repositories.models.listByProvider(row.id, selectedOnly);
    const account = row.accountId === null ? undefined : this.#account(row.accountId);
    return {
      ...toSummaryDto(this.#accountStatus(row), models.length),
      baseUrl: row.baseUrl,
      secretId: row.secretId as SecretId | null,
      secretName: row.secretId === null ? null : (this.#secretSummary(row.secretId)?.name ?? null),
      accountId: row.accountId as AccountId | null,
      accountLabel: accountLabel(account),
      settings: { ...row.settings },
      adapterCapabilities: toCapabilitiesDto(adapterCapabilities(row.adapter)),
      models: models.map(toModelDto),
    };
  }

  #countModels(providerId: string): number {
    return this.#repositories.models.listByProvider(providerId).length;
  }

  #accountStatus(row: ProviderEntity): ProviderEntity {
    if (row.accountId === null) return row;
    const account = this.#account(row.accountId);
    if (account?.status === "linked") return row;
    const derived = deriveStatus({
      outcome: { kind: "session-expired" },
      now: timestampNow(this.#clock),
    });
    return { ...row, status: derived.status, statusDetail: derived.detail };
  }

  #require(id: string): ProviderEntity {
    const row = this.#repositories.providers.findById(id);
    if (row === undefined) throw notFound(id);
    return row;
  }

  get #repositories(): Repositories {
    return this.#data.repositories;
  }
}

function discoveryPort(driver: AiDriver, providerId: string) {
  const port = driver.text ?? driver.embedding ?? driver.image;
  if (port === null) {
    throw new AppError(AppErrorCode.CONFLICT, "Провайдер не предоставляет список моделей", {
      details: { providerId },
    });
  }
  return port;
}

function latencyOf(outcome: ProbeOutcome): number | null {
  return outcome.kind === "ok" || outcome.kind === "ok-empty" ? outcome.latencyMs : null;
}

function discoveredRows(outcome: ProbeOutcome, discoveredAt: Timestamp) {
  const models: readonly DiscoveredModel[] = outcome.kind === "ok" ? outcome.models : [];
  return models.map((entry) => ({
    isFree: entry.isFree ?? null,
    noTraining: entry.noTraining ?? null,
    externalId: entry.externalId,
    displayName: entry.displayName,
    family: entry.family,
    contextWindow: entry.contextWindow,
    maxOutput: entry.maxOutput,
    sizeBytes: entry.sizeBytes,
    capabilities: [...entry.capabilities],
    available: true,
    unavailableReason: null,
    discoveredAt,
  }));
}

function connectionColumns(input: ProviderDraftInput) {
  return {
    kind: input.kind,
    adapter: input.adapter,
    authMode: input.authMode,
    baseUrl: input.baseUrl,
    secretId: input.secretId,
    accountId: input.accountId,
    capabilities: [...input.capabilities],
    settings: { ...input.settings } as ProviderSettings,
  };
}

function draftRow(draft: ProviderDraftInput, now: Timestamp): ProviderEntity {
  return {
    ...connectionColumns(draft),
    id: `draft-${createId()}`,
    name: draft.name ?? "draft",
    capText: draft.capabilities.includes("text"),
    capEmbedding: draft.capabilities.includes("embedding"),
    capImage: draft.capabilities.includes("image"),
    enabled: true,
    defaultModelId: null,
    status: "unknown",
    statusDetail: null,
    lastProbeAt: null,
    lastLatencyMs: null,
    createdAt: now,
    updatedAt: now,
  };
}

function accountLabel(account: AccountEntity | undefined): string | null {
  if (account === undefined) return null;
  return account.emailMasked ?? account.displayName ?? "Связанный аккаунт";
}

function toCapabilitiesDto(capabilities: AdapterCapabilities): ProviderDto["adapterCapabilities"] {
  return {
    family: capabilities.family,
    authModes: [...capabilities.authModes],
    streaming: capabilities.streaming,
    liveModelList: capabilities.liveModelList,
    embedding: capabilities.embedding,
    image: capabilities.image,
    honours: { ...capabilities.honours },
  };
}

export function toSummaryDto(row: ProviderEntity, modelCount: number): ProviderSummaryDto {
  return {
    id: row.id as ProviderId,
    kind: row.kind,
    adapter: row.adapter,
    authMode: row.authMode,
    name: row.name,
    capabilities: [...row.capabilities],
    enabled: row.enabled,
    status: row.status,
    statusDetail: row.statusDetail,
    lastProbeAt: row.lastProbeAt,
    lastLatencyMs: row.lastLatencyMs,
    modelCount,
    defaultModelId: row.defaultModelId as ModelId | null,
    updatedAt: row.updatedAt,
  };
}

export function toModelDto(row: ModelEntity): ModelDto {
  return {
    isFree: row.isFree,
    noTraining: row.noTraining,
    id: row.id as ModelId,
    providerId: row.providerId as ProviderId,
    externalId: row.externalId,
    displayName: row.displayName,
    family: row.family,
    contextWindow: row.contextWindow,
    maxOutput: row.maxOutput,
    sizeBytes: row.sizeBytes,
    capabilities: [...row.capabilities],
    available: row.available,
    unavailableReason: row.unavailableReason,
    discoveredAt: row.discoveredAt,
  };
}

export function toProbeResultDto(result: ProbeResult): ProbeResultDto {
  return {
    providerId: result.providerId as ProviderId | null,
    outcome: toOutcomeDto(result.outcome),
    status: result.derived.status,
    statusDetail: result.derived.detail,
    checkedAt: result.checkedAt,
    provider: result.provider,
  };
}

function toOutcomeDto(outcome: ProbeOutcome): ProbeResultDto["outcome"] {
  if (outcome.kind !== "ok") return { ...outcome };
  return {
    kind: "ok",
    latencyMs: outcome.latencyMs,
    live: outcome.live,
    models: outcome.models.map((entry) => ({
      isFree: entry.isFree ?? null,
      noTraining: entry.noTraining ?? null,
      externalId: entry.externalId,
      displayName: entry.displayName,
      family: entry.family,
      contextWindow: entry.contextWindow,
      maxOutput: entry.maxOutput,
      sizeBytes: entry.sizeBytes,
      capabilities: [...entry.capabilities],
    })),
  };
}

function notFound(providerId: string): AppError {
  return new AppError(AppErrorCode.NOT_FOUND, "Провайдер не найден", { details: { providerId } });
}
