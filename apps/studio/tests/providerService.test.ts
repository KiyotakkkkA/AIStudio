import { readFileSync } from "node:fs";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import {
  AppError,
  AppErrorCode,
  CreateProviderInput,
  ProbeResultDto,
  timestampNow,
} from "@zvs/shared";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { createSecretService } from "../../../test/helpers/secretService.ts";
import { createFakeDriver } from "../../../test/helpers/FakeDriver.ts";
import { ProviderService, toProbeResultDto } from "../src/host/services/ProviderService.ts";
import { deriveStatus, DAY_MS } from "../src/host/services/providerStatus.ts";
import { failureOutcome, type ProbeOutcome } from "../src/host/services/probeOutcome.ts";
import {
  HealthCheckService,
  HEALTH_CHECK_ENABLED_KEY,
  HEALTH_CHECK_INTERVAL_KEY,
} from "../src/host/services/HealthCheckService.ts";
import { SettingService } from "../src/host/services/SettingService.ts";
import { createEventBus } from "../src/host/platform/events.ts";
import { createProviderHandlers } from "../src/host/ipc/providers.ts";
import { createSettingHandlers } from "../src/host/ipc/settings.ts";
import { ProviderRegistry } from "../src/host/drivers/ai/ProviderRegistry.ts";
import { probeAccountCredentials } from "../src/host/drivers/ai/identity/ProbeAccountCredentials.ts";

const now = 1_800_000_000_000;

test("selected models and filter metadata survive saving and rediscovery", async () => {
  const selectedModelIds = ["model-1", "model-2"];
  const row = service.create({ ...input, settings: { selectedModelIds } });
  driver.script({ models: [{ ...model, isFree: true, noTraining: true }] });
  await service.probe({ id: row.id });
  const saved = service.get(row.id);
  expect(saved.settings.selectedModelIds).toEqual(selectedModelIds);
  expect(saved.models[0]).toMatchObject({ isFree: true, noTraining: true });
  expect(
    ProbeResultDto.parse(toProbeResultDto(await service.probe({ id: row.id }))).provider?.models[0],
  ).toMatchObject({ isFree: true, noTraining: true });
  service.update({ id: row.id, settings: { selectedModelIds: [] } });
  expect(service.get(row.id).settings.selectedModelIds).toEqual([]);
});
const model = {
  externalId: "model-1",
  displayName: "Model One",
  family: null,
  contextWindow: 8192,
  maxOutput: 1024,
  sizeBytes: null,
  capabilities: [],
};
const input = CreateProviderInput.parse({
  kind: "ollama",
  name: "Local",
  baseUrl: "http://localhost:11434",
  capabilities: ["text"],
});
let database: TemporaryDatabase;
let driver: ReturnType<typeof createFakeDriver>;
let service: ProviderService;
let secrets: ReturnType<typeof createSecretService>;
const log = vi.fn();

beforeEach(() => {
  database = temporaryDatabase();
  secrets = createSecretService(database.client, () => now);
  driver = createFakeDriver({ models: [model] });
  service = new ProviderService({
    data: database.client,
    secrets,
    drivers: { ephemeralDriver: async () => driver, invalidate() {} },
    clock: () => now,
    logger: { log, close() {} },
  });
  log.mockClear();
});

afterEach(async () => {
  await service.dispose();
  database.dispose();
  vi.useRealTimers();
});

test("discovery replaces stale models and preserves or clears the default atomically", async () => {
  const row = service.create(input);
  const first = await service.probe({ id: row.id });
  expect(first.outcome.kind).toBe("ok");
  expect(first.provider?.lastProbeAt).toBe(now);
  expect(first.provider?.lastLatencyMs).toBe(0);
  const selected = service.models(row.id)[0]!;
  service.setDefaultModel(row.id, selected.id);
  driver.script({ models: [model, { ...model, externalId: "stale" }] });
  await service.probe({ id: row.id });
  expect(service.get(row.id).defaultModelId).toBe(selected.id);
  driver.script({ models: [{ ...model, externalId: "replacement" }] });
  await service.probe({ id: row.id });
  expect(service.models(row.id).map((entry) => entry.externalId)).toEqual(["replacement"]);
  expect(service.get(row.id).defaultModelId).toBeNull();
  expect(log.mock.calls.some((call) => call[2] === "Default model vanished from the vendor")).toBe(
    true,
  );
  const before = service.get(row.id);
  driver.script({ models: [model, model] });
  await expect(service.probe({ id: row.id })).rejects.toThrow();
  expect(service.get(row.id)).toEqual(before);
});

test("empty success clears discovery while failed probes retain models", async () => {
  const row = service.create(input);
  await service.probe({ id: row.id });
  driver.script({ failWith: new AppError(AppErrorCode.PROVIDER_AUTH_FAILED, "private payload") });
  const failure = await service.probe({ id: row.id });
  expect(failure.derived.status).toBe("failed");
  expect(service.models(row.id)).toHaveLength(1);
  driver = createFakeDriver({ models: [] });
  const empty = await service.probe({ id: row.id });
  expect(empty.outcome.kind).toBe("ok-empty");
  expect(empty.derived.status).toBe("ok");
  expect(service.models(row.id)).toEqual([]);
});

test.each([
  [AppErrorCode.PROVIDER_AUTH_FAILED, "auth-failed", {}],
  [AppErrorCode.PROVIDER_SESSION_EXPIRED, "session-expired", {}],
  [AppErrorCode.PROVIDER_SESSION_EXPIRED, "account-not-linked", { reason: "account-not-linked" }],
  [AppErrorCode.PROVIDER_UNREACHABLE, "unreachable", {}],
  [AppErrorCode.RATE_LIMITED, "rate-limited", { retryAfter: 12 }],
  [AppErrorCode.UNKNOWN, "error", {}],
] as const)("maps %s to %s without exposing vendor messages", async (code, kind, details) => {
  driver.script({ failWith: new AppError(code, "private payload", { details }) });
  const result = toProbeResultDto(await service.probe({ draft: input }));
  expect(result.outcome.kind).toBe(kind);
  expect(result.status).toBe("failed");
  expect(result.statusDetail).toBeTruthy();
  expect(JSON.stringify(result)).not.toContain("private payload");
  expect(ProbeResultDto.safeParse(result).success).toBe(true);
});

test("credential lifetime warnings are pure and auth failures have distinct actions", () => {
  const derive = (outcome: ProbeOutcome) => deriveStatus({ outcome, now: timestampNow(() => now) });
  expect(derive({ kind: "auth-failed" }).detail).not.toBe(
    derive({ kind: "session-expired" }).detail,
  );
  for (const source of ["secret", "account"] as const) {
    for (const days of [-1, 2]) {
      const result = deriveStatus({
        outcome: { kind: "ok-empty", latencyMs: 1 },
        now: timestampNow(() => now),
        lifetimes: [{ source, expiresAt: timestampNow(() => now + days * DAY_MS) }],
      });
      expect(result.status).toBe("degraded");
      expect(result.detail).toBeTruthy();
    }
  }
  expect(failureOutcome(new Error("private payload"))).toEqual({
    kind: "error",
    code: AppErrorCode.UNKNOWN,
    detail: "Не удалось выполнить проверку",
  });
});

test("draft discovery does not write and DTOs contain references only", async () => {
  const secret = secrets.create({
    type: "openrouter",
    name: "Main key",
    scope: "personal",
    value: "sk-secret-value",
  });
  const draft = CreateProviderInput.parse({ ...input, secretId: secret.id });
  const before = database.client.repositories.providers.list();
  const result = toProbeResultDto(await service.probe({ draft }));
  expect(result.providerId).toBeNull();
  expect(result.provider).toBeNull();
  expect(database.client.repositories.providers.list()).toEqual(before);
  const saved = service.create(draft);
  expect(saved.secretName).toBe("Main key");
  expect(JSON.stringify(saved)).not.toContain("sk-secret-value");
  expect(saved.status).toBe("unknown");
});

test("account expiry uses seconds from the account row and displays a masked identity", async () => {
  const account = database.client.repositories.accounts.create({
    adapter: "qwen-web",
    externalId: "private-id",
    emailMasked: "a***@example.com",
    partition: "persist:browser-work",
    tokenExpiresAt: (now + 2 * DAY_MS) / 1000,
    linkedAt: now,
    updatedAt: now,
  });
  const row = database.client.repositories.providers.create({
    ...input,
    authMode: "account",
    adapter: "qwen-web",
    accountId: account.id,
    createdAt: now,
    updatedAt: now,
  });
  const result = await service.probe({ id: row.id });
  expect(result.derived).toEqual({ status: "degraded", detail: "Сессия истекает через 2 дн." });
  expect(result.provider?.accountLabel).toBe("a***@example.com");
});

test("registry distinguishes unlinked and expired accounts without calling a vendor", async () => {
  const registry = new ProviderRegistry({
    providers: database.client.repositories.providers,
    accounts: database.client.repositories.accounts,
    secrets,
  });
  const actual = new ProviderService({ data: database.client, drivers: registry, secrets });
  const draft = { ...input, authMode: "account" as const, adapter: "qwen-web" as const };
  expect((await actual.probe({ draft })).outcome.kind).toBe("account-not-linked");
  const account = database.client.repositories.accounts.create({
    adapter: "qwen-web",
    externalId: "id",
    partition: "persist:browser-work",
    status: "needs-relink",
    linkedAt: now,
    updatedAt: now,
  });
  const row = database.client.repositories.providers.create({
    ...draft,
    accountId: account.id,
    createdAt: now,
    updatedAt: now,
  });
  expect((await actual.probe({ id: row.id })).outcome.kind).toBe("session-expired");
  await actual.dispose();
});

test("concurrent probes join and connection mutations cannot race discovery", async () => {
  const row = service.create(input);
  driver.script({ manual: true });
  const first = service.probe({ id: row.id });
  const second = service.probe({ id: row.id });
  expect(second).toBe(first);
  await driver.whenHolding();
  expect(() => service.update({ id: row.id, baseUrl: "http://elsewhere" })).toThrow();
  expect(() => service.remove(row.id)).toThrow();
  driver.release();
  await first;
  expect(service.models(row.id)).toHaveLength(1);
  expect(service.isProbing(row.id)).toBe(false);
});

test("probe timeout includes a stalled driver factory", async () => {
  vi.useFakeTimers();
  const actual = new ProviderService({
    data: database.client,
    secrets,
    probeTimeoutSeconds: 1,
    drivers: { ephemeralDriver: () => new Promise(() => undefined), invalidate() {} },
  });
  const pending = actual.probe({ draft: input });
  await vi.advanceTimersByTimeAsync(1000);
  expect((await pending).outcome.kind).toBe("unreachable");
  await actual.dispose();
});

test("health checking is opt-in, configurable, skips busy providers and stops pending work", async () => {
  vi.useFakeTimers();
  const settings = new SettingService({ data: database.client });
  const events = createEventBus();
  const health = new HealthCheckService({ providers: service, settings, events });
  const settingsHandlers = createSettingHandlers(settings, health);
  expect(health.start()).toBe(false);
  await health.runOnce();
  expect(health.intervalMinutes).toBe(15);
  await settingsHandlers["settings.set"]({ key: HEALTH_CHECK_INTERVAL_KEY, value: 2 });
  await settingsHandlers["settings.set"]({ key: HEALTH_CHECK_ENABLED_KEY, value: true });
  expect(health.running).toBe(true);
  expect(health.intervalMinutes).toBe(2);
  const row = service.create(input);
  service.create({ ...input, name: "Second" });
  driver.script({ manual: true });
  const manual = service.probe({ id: row.id });
  await driver.whenHolding();
  service.update({ id: service.list().find((entry) => entry.id !== row.id)!.id, enabled: false });
  await health.runOnce();
  expect(service.isProbing(row.id)).toBe(true);
  driver.release();
  await manual;
  const pending = health.runOnce();
  await driver.whenHolding();
  health.stop();
  await pending;
  await health.dispose();
  expect(health.running).toBe(false);
  expect(events.open).toBe(0);
  expect(service.isProbing(row.id)).toBe(false);
  await vi.advanceTimersByTimeAsync(30 * 60 * 1000);
  expect(service.isProbing(row.id)).toBe(false);
  events.dispose();
});

test("provider handlers support CRUD, filters, discovery and defaults", async () => {
  const handlers = createProviderHandlers(service);
  const row = await handlers["providers.create"](input);
  expect(await handlers["providers.list"]({ capability: "image" })).toEqual([]);
  const result = await handlers["providers.probe"]({ id: row.id });
  expect(ProbeResultDto.safeParse(result).success).toBe(true);
  const selected = result.provider!.models[0]!;
  expect(
    (await handlers["providers.setDefaultModel"]({ id: row.id, modelId: selected.id }))
      .defaultModelId,
  ).toBe(selected.id);
  expect((await handlers["providers.update"]({ id: row.id, name: "Renamed" })).name).toBe(
    "Renamed",
  );
  expect(await handlers["providers.refreshAll"](undefined)).toHaveLength(1);
  expect((await handlers["providers.get"]({ id: row.id })).name).toBe("Renamed");
  await handlers["providers.remove"]({ id: row.id });
  expect(service.list()).toEqual([]);
});

test("ProviderService stays auth-mode agnostic", () => {
  const source = readFileSync(
    new URL("../src/host/services/ProviderService.ts", import.meta.url),
    "utf8",
  );
  expect(source).not.toMatch(/switch\s*\([^)]*auth_?mode/i);
  expect(source).not.toMatch(/auth_?mode\s*(?:===|!==|==|!=)\s*["'](?:api|account)["']/i);
});

test("account draft credentials verify identity without changing the database", async () => {
  const account = database.client.repositories.accounts.create({
    adapter: "qwen-web",
    externalId: "same-id",
    partition: "persist:browser-work",
    linkedAt: now,
    updatedAt: now,
  });
  const probe = {
    family: "qwen-web" as const,
    endpoint: "https://example.com/identity",
    loginUrl: "https://example.com/",
    probe: vi.fn(async () => ({
      identity: { externalId: "same-id" },
      credential: { token: "private-token", tokenType: "Bearer" },
    })),
  };
  const credentials = probeAccountCredentials(
    account,
    {
      partition: account.partition,
      userAgent: () => "test",
      fetch: async () => {
        throw new Error("Unexpected network call");
      },
    },
    secrets,
    probe,
  );
  const actual = new ProviderService({
    data: database.client,
    secrets,
    drivers: {
      ephemeralDriver: async () => ({
        ...driver,
        text: {
          ...driver.text,
          listModels: async (signal) => {
            await credentials.current(signal);
            return [model];
          },
        },
      }),
      invalidate() {},
    },
  });
  const draft = CreateProviderInput.parse({
    ...input,
    adapter: "qwen-web",
    authMode: "account",
    accountId: account.id,
  });
  const result = toProbeResultDto(await actual.probe({ draft }));
  expect(result.outcome.kind).toBe("ok");
  expect(JSON.stringify(result)).not.toContain("private-token");
  expect(database.client.repositories.accounts.getById(account.id)).toEqual(account);
  expect(secrets.list()).toEqual([]);
  expect(service.list()).toEqual([]);
  probe.probe.mockResolvedValue({
    identity: { externalId: "different-id" },
    credential: { token: "private-token", tokenType: "Bearer" },
  });
  expect((await actual.probe({ draft })).outcome.kind).toBe("session-expired");
  expect(database.client.repositories.accounts.getById(account.id)).toEqual(account);
  await actual.dispose();
});

test("saved probes apply secret rotation and mark curated model lists", async () => {
  const secret = secrets.create({
    type: "openrouter",
    name: "Expiring",
    scope: "personal",
    value: "sk-key",
    rotatesAt: timestampNow(() => now - DAY_MS),
  });
  const row = service.create(CreateProviderInput.parse({ ...input, secretId: secret.id }));
  expect((await service.probe({ id: row.id })).derived.status).toBe("degraded");
  const result = await service.probe({
    draft: { ...input, adapter: "deepseek-web", authMode: "account" },
  });
  expect(result.outcome).toMatchObject({ kind: "ok", live: false });
});
