import { afterEach, beforeEach, expect, test, vi } from "vitest";
import {
  AccountDto,
  AccountLinkResult,
  AppError,
  AppErrorCode,
  HostEvent,
  contract,
} from "@zvs/shared";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { createSecretService } from "../../../test/helpers/secretService.ts";
import { createProviderService } from "../../../test/helpers/providerService.ts";
import { AccountService } from "../src/host/services/AccountService.ts";
import type { ProbeResult } from "../src/host/drivers/ai/identity/IdentityProbe.ts";
import { BrowserLifecycle } from "../src/host/browser/lifecycle.ts";
import { createEventBus } from "../src/host/platform/events.ts";
import { createAccountHandlers } from "../src/host/ipc/accounts.ts";

let database: TemporaryDatabase;
let service: AccountService;
let lifecycle: BrowserLifecycle;
let events: HostEvent[];
let calls: AbortSignal[];
let probe: ReturnType<typeof vi.fn<(signal: AbortSignal) => Promise<ProbeResult>>>;
let opener: ReturnType<typeof vi.fn<(url: string) => void>>;
let linked: ReturnType<typeof vi.fn<() => void>>;

const signedOut = () => new AppError(AppErrorCode.PROVIDER_SESSION_EXPIRED, "Signed out");
const identity = (id = "user-1", token = "private-bearer-token"): ProbeResult => ({
  identity: {
    externalId: id,
    emailMasked: "person@example.com",
    displayName: "Person",
    expiresAt: 2_000_000_000,
  },
  credential: { token, tokenType: "Bearer" },
});

beforeEach(() => {
  vi.useFakeTimers();
  database = temporaryDatabase();
  lifecycle = new BrowserLifecycle();
  events = [];
  calls = [];
  opener = vi.fn();
  linked = vi.fn();
  probe = vi.fn(async () => identity());
  service = new AccountService({
    data: database.client,
    secrets: createSecretService(database.client),
    events: createEventBus({
      senders: () => [
        {
          isDestroyed: () => false,
          send: (_channel, event) => {
            events.push(HostEvent.parse(event));
          },
        },
      ],
    }),
    lifecycle,
    browser: { openTab: opener },
    sessions: (partition) => ({
      partition,
      userAgent: () => "test",
      fetch: async () => {
        throw new Error("Unexpected network call");
      },
    }),
    probes: (family) => ({
      family,
      endpoint: "https://example.com/identity",
      loginUrl: family === "qwen-web" ? "https://chat.qwen.ai/" : "https://chat.deepseek.com/",
      probe: (_session, signal) => {
        calls.push(signal);
        return probe(signal);
      },
    }),
    timeoutMs: 6000,
    intervalMs: 2000,
    random: () => 0.5,
    onLinked: linked,
  });
});

afterEach(async () => {
  await service.dispose();
  database.dispose();
  vi.useRealTimers();
});

test("polling stores a masked identity and encrypted token with progress and done events", async () => {
  probe.mockRejectedValueOnce(signedOut());
  const pending = service.link({ adapter: "qwen-web" });
  await vi.advanceTimersByTimeAsync(0);
  expect(opener).toHaveBeenCalledWith("https://chat.qwen.ai/");
  expect(service.list()).toEqual([]);
  await vi.advanceTimersByTimeAsync(2000);
  const result = AccountLinkResult.parse(await pending);
  expect(result.status).toBe("linked");
  const dto = service.list()[0]!;
  expect(dto.emailMasked).toBe("p***@example.com");
  expect(dto.expiresAt).toBe(2_000_000_000);
  const row = database.client.repositories.accounts.getById(dto.id)!;
  expect(await createSecretService(database.client).resolve(row.tokenSecretId!)).toBe(
    "private-bearer-token",
  );
  expect(events.some((event) => event.type === "progress")).toBe(true);
  expect(events.some((event) => event.type === "step" && event.step.phase === "done")).toBe(true);
  expect(events.at(-1)).toMatchObject({ type: "end", outcome: { status: "ok" } });
  expect(JSON.stringify({ result, events, list: service.list() })).not.toMatch(
    /private-bearer|tokenSecret|partition|person@example/,
  );
  expect(linked).toHaveBeenCalledOnce();
  expect(vi.getTimerCount()).toBe(0);
});

test("timeout writes nothing and makes no further requests", async () => {
  probe.mockRejectedValue(signedOut());
  const pending = service.link({ adapter: "deepseek-web" });
  await vi.advanceTimersByTimeAsync(6000);
  expect(await pending).toMatchObject({ status: "timeout" });
  expect(service.list()).toEqual([]);
  expect(createSecretService(database.client).list()).toEqual([]);
  const count = probe.mock.calls.length;
  await vi.advanceTimersByTimeAsync(60_000);
  expect(probe).toHaveBeenCalledTimes(count);
  expect(vi.getTimerCount()).toBe(0);
});

test.each(["explicit", "tab", "workspace", "dispose"] as const)(
  "%s cancellation aborts an in-flight request even if the probe ignores abort",
  async (mode) => {
    let resolve: (result: ProbeResult) => void = () => undefined;
    probe.mockImplementation(
      () =>
        new Promise((done) => {
          resolve = done;
        }),
    );
    const pending = service.link({ adapter: "qwen-web" });
    await vi.advanceTimersByTimeAsync(0);
    if (mode === "explicit") service.cancelLink({ adapter: "qwen-web" });
    if (mode === "tab") lifecycle.emit({ type: "link-tab-closed", url: "https://chat.qwen.ai/" });
    if (mode === "workspace") lifecycle.emit({ type: "workspace-closed" });
    if (mode === "dispose") await service.dispose();
    expect(await pending).toMatchObject({ status: "cancelled" });
    expect(calls[0]?.aborted).toBe(true);
    resolve(identity());
    await vi.advanceTimersByTimeAsync(60_000);
    expect(service.list()).toEqual([]);
    expect(probe).toHaveBeenCalledOnce();
    expect(vi.getTimerCount()).toBe(0);
  },
);

test("cancel during the polling delay clears the timer immediately", async () => {
  probe.mockRejectedValue(signedOut());
  const pending = service.link({ adapter: "qwen-web" });
  await vi.advanceTimersByTimeAsync(0);
  service.cancelLink({ adapter: "qwen-web" });
  await pending;
  expect(vi.getTimerCount()).toBe(0);
  await vi.advanceTimersByTimeAsync(60_000);
  expect(probe).toHaveBeenCalledOnce();
});

test("duplicate linking joins one loop and cancelling before it starts opens nothing", async () => {
  const first = service.link({ adapter: "qwen-web" });
  expect(service.link({ adapter: "qwen-web" })).toBe(first);
  service.cancelLink({ adapter: "qwen-web" });
  expect(await first).toMatchObject({ status: "cancelled" });
  expect(opener).not.toHaveBeenCalled();
  expect(probe).not.toHaveBeenCalled();
});

function attach(accountId: string) {
  return database.client.repositories.providers.create({
    kind: "openai-compatible",
    adapter: "qwen-web",
    authMode: "account",
    name: "Qwen",
    accountId,
    baseUrl: "https://chat.qwen.ai",
    capabilities: ["text"],
    settings: { temperature: 0.7 },
    createdAt: Date.now(),
    updatedAt: Date.now(),
  });
}

test("same identity retains its row, provider settings and default model", async () => {
  await service.link({ adapter: "qwen-web" });
  const row = service.list()[0]!;
  const provider = attach(row.id);
  const models = database.client.repositories.models.replaceForProvider(provider.id, [
    {
      externalId: "qwen",
      displayName: "Qwen",
      family: null,
      contextWindow: null,
      maxOutput: null,
      sizeBytes: null,
      capabilities: [],
      available: true,
      unavailableReason: null,
      discoveredAt: Date.now(),
    },
  ]);
  database.client.repositories.providers.setDefaultModel(provider.id, models[0]!.id);
  const before = database.client.repositories.providers.findById(provider.id);
  probe.mockResolvedValue(identity("user-1", "replacement-token"));
  const result = await service.link({ adapter: "qwen-web" });
  expect(result).toMatchObject({
    status: "linked",
    replacedPreviousIdentity: false,
    account: { id: row.id, linkedProvidersCount: 1 },
  });
  expect(database.client.repositories.providers.findById(provider.id)).toEqual(before);
  expect(createSecretService(database.client).list()).toHaveLength(1);
});

test("different identity replaces the previous account and deletes its token", async () => {
  await service.link({ adapter: "qwen-web" });
  const old = service.list()[0]!;
  const provider = attach(old.id);
  probe.mockResolvedValue(identity("user-2", "new-identity-token"));
  const result = await service.link({ adapter: "qwen-web" });
  expect(result).toMatchObject({ status: "linked", replacedPreviousIdentity: true });
  expect(service.list()).toHaveLength(1);
  expect(database.client.repositories.accounts.getById(old.id)).toBeUndefined();
  expect(database.client.repositories.providers.findById(provider.id)).toMatchObject({
    accountId: null,
    status: "failed",
    statusDetail: "no account linked",
  });
  expect(createSecretService(database.client).list()).toHaveLength(1);
});

test("unlink preserves browser session and providers while deleting the token", async () => {
  await service.link({ adapter: "qwen-web" });
  const row = service.list()[0]!;
  const provider = attach(row.id);
  expect(service.unlink(row.id)).toMatchObject({ removed: true, browserSessionPreserved: true });
  expect(service.list()).toEqual([]);
  expect(database.client.repositories.providers.findById(provider.id)).toMatchObject({
    accountId: null,
    status: "failed",
  });
  expect(createSecretService(database.client).list()).toEqual([]);
  expect(opener).toHaveBeenCalledOnce();
});

test("cookie clearing marks only the matching vendor and derives session-expired without outbound calls", async () => {
  await service.link({ adapter: "qwen-web" });
  await service.link({ adapter: "deepseek-web" });
  const row = service.list().find((account) => account.adapter === "qwen-web")!;
  const provider = attach(row.id);
  lifecycle.emit({ type: "cookies-cleared", domain: "evilqwen.ai" });
  expect(service.list().find((account) => account.id === row.id)?.status).toBe("linked");
  lifecycle.emit({ type: "cookies-cleared", domain: ".qwen.ai" });
  expect(service.list().find((account) => account.id === row.id)?.status).toBe("needs-relink");
  expect(service.list().find((account) => account.adapter === "deepseek-web")?.status).toBe(
    "linked",
  );
  const providers = createProviderService(database.client);
  expect(providers.get(provider.id)).toMatchObject({
    status: "failed",
    statusDetail: "Сессия истекла — войдите заново",
  });
  expect((await providers.probe({ id: provider.id })).outcome).toEqual({ kind: "session-expired" });
  expect(probe).toHaveBeenCalledTimes(2);
  await providers.dispose();
});

test("refresh updates health, expiry and token, and rejects mismatched identities", async () => {
  await service.link({ adapter: "qwen-web" });
  const row = service.list()[0]!;
  probe.mockRejectedValueOnce(signedOut());
  expect(await service.refresh(row.id)).toMatchObject({ status: "needs-relink" });
  probe.mockResolvedValueOnce(identity("other-user"));
  expect(await service.refresh(row.id)).toMatchObject({ status: "needs-relink" });
  probe.mockResolvedValueOnce({
    identity: { externalId: "user-1", expiresAt: 3_000_000_000 },
    credential: null,
  });
  expect(await service.refresh(row.id)).toMatchObject({
    status: "linked",
    expiresAt: 3_000_000_000,
  });
  expect(createSecretService(database.client).list()).toEqual([]);
});

test("cookie clearing aborts a refresh so a late response cannot restore linked status", async () => {
  await service.link({ adapter: "qwen-web" });
  const row = service.list()[0]!;
  let resolve: (result: ProbeResult) => void = () => undefined;
  probe.mockImplementation(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  const pending = service.refresh(row.id);
  const rejected = expect(pending).rejects.toBeDefined();
  lifecycle.emit({ type: "cookies-cleared", domain: "chat.qwen.ai" });
  await rejected;
  resolve(identity());
  await vi.advanceTimersByTimeAsync(0);
  expect(service.list()[0]?.status).toBe("needs-relink");
});

test("all account channel outputs validate and contain no bearer token", async () => {
  const handlers = createAccountHandlers(service);
  const result = await handlers["accounts.link"]({ adapter: "qwen-web" });
  expect(contract["accounts.link"].output.safeParse(result).success).toBe(true);
  const list = await handlers["accounts.list"]();
  expect(contract["accounts.list"].output.safeParse(list).success).toBe(true);
  const account = AccountDto.parse(list[0]);
  expect(
    contract["accounts.refresh"].output.safeParse(
      await handlers["accounts.refresh"]({ id: account.id }),
    ).success,
  ).toBe(true);
  expect(
    contract["accounts.cancelLink"].output.safeParse(
      await handlers["accounts.cancelLink"]({ adapter: "qwen-web" }),
    ).success,
  ).toBe(true);
  expect(
    contract["accounts.unlink"].output.safeParse(
      await handlers["accounts.unlink"]({ id: account.id }),
    ).success,
  ).toBe(true);
  expect(JSON.stringify({ result, list, events })).not.toContain("private-bearer-token");
});
