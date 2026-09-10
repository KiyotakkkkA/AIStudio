import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { afterEach, beforeEach, test } from "vitest";
import { AppErrorCode, isAppError } from "@zvs/shared";
import { createFakeSession, FAKE_USER_AGENT } from "../../../test/helpers/FakeSession.ts";
import { createSecretService } from "../../../test/helpers/secretService.ts";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import type {
  AccountDraft,
  ProviderDraft,
  Repositories,
} from "../src/host/data/repositories/index.ts";
import type { LogLevel, Logger } from "../src/host/platform/logger.ts";
import { AccountCredentialStore } from "../src/host/drivers/ai/identity/AccountCredentialStore.ts";
import {
  DEEPSEEK_IDENTITY_ENDPOINT,
  mapDeepSeekIdentity,
} from "../src/host/drivers/ai/identity/deepseek.ts";
import { IDENTITY_PROBES, identityProbe } from "../src/host/drivers/ai/identity/registry.ts";
import { QWEN_IDENTITY_ENDPOINT, mapQwenIdentity } from "../src/host/drivers/ai/identity/qwen.ts";
import { AccountTransport } from "../src/host/drivers/ai/transport/AccountTransport.ts";
import { ProviderRegistry } from "../src/host/drivers/ai/ProviderRegistry.ts";
import type { SecretService } from "../src/host/services/SecretService.ts";

const HERE = dirname(fileURLToPath(import.meta.url));

let database: TemporaryDatabase;
let repositories: Repositories;
let secrets: SecretService;

const accountDraft: AccountDraft = {
  adapter: "deepseek-web",
  partition: "persist:browser-work",
  externalId: "user-1",
  linkedAt: 100,
  updatedAt: 100,
};

const providerDraft: ProviderDraft = {
  kind: "openai-compatible",
  name: "DeepSeek",
  adapter: "deepseek-web",
  authMode: "account",
  baseUrl: "https://chat.deepseek.com/api/v0",
  capabilities: ["text"],
  createdAt: 100,
  updatedAt: 100,
};

const qwenPayload = {
  id: "qwen-9",
  email: "k***@mail.ru",
  name: "Кирилл",
  profile_image_url: "data:image/png;base64,AAAA",
  token: "qwen-token-1",
  token_type: "Bearer",
  expires_at: 4_102_444_800,
};

const deepSeekPayload = {
  code: 0,
  msg: "",
  data: {
    biz_code: 0,
    biz_msg: "",
    biz_data: {
      id: 4242,
      email: "k***@mail.ru",
      token: "deepseek-token-1",
      id_profile: { name: "Кирилл", picture: "https://cdn.deepseek.com/a.png" },
    },
  },
};

beforeEach(() => {
  database = temporaryDatabase();
  repositories = database.client.repositories;
  secrets = createSecretService(database.client);
});

afterEach(() => database.dispose());

interface CapturingLogger extends Logger {
  readonly lines: readonly string[];
}

function capturingLogger(): CapturingLogger {
  const lines: string[] = [];
  return {
    lines,
    log(level: LogLevel, scope: string, message: string, fields?: Record<string, unknown>): void {
      lines.push(JSON.stringify({ level, scope, message, fields }));
    },
    close(): void {},
  };
}

function code(error: unknown): string {
  return isAppError(error) ? error.code : `not-an-app-error: ${String(error)}`;
}

async function rejection(run: () => Promise<unknown>): Promise<unknown> {
  try {
    await run();
  } catch (error: unknown) {
    return error;
  }
  throw new Error("Expected the call to reject");
}

test("migration 0004 adds the account table and the provider link", () => {
  const connection = database.client.db.$client;
  const columns = connection.prepare("PRAGMA table_info(account)").all() as {
    name: string;
    notnull: number;
    dflt_value: string | null;
  }[];
  const byName = new Map(columns.map((column) => [column.name, column]));
  assert.deepEqual(columns.map((column) => column.name).sort(), [
    "adapter",
    "avatar_url",
    "display_name",
    "email_masked",
    "external_id",
    "id",
    "last_checked_at",
    "linked_at",
    "partition",
    "status",
    "status_detail",
    "token_expires_at",
    "token_secret_id",
    "updated_at",
  ]);
  assert.equal(byName.get("status")?.dflt_value, "'linked'");
  assert.equal(byName.get("linked_at")?.notnull, 1);

  const providerColumns = connection.prepare("PRAGMA table_info(provider)").all() as {
    name: string;
  }[];
  assert.equal(
    providerColumns.some((column) => column.name === "account_id"),
    true,
  );

  const linked = repositories.accounts.create(accountDraft);
  assert.throws(() => repositories.accounts.create({ ...accountDraft, id: undefined }), /UNIQUE/);
  assert.equal(linked.status, "linked");
  assert.deepEqual(repositories.accounts.listByAdapter("deepseek-web"), [linked]);
  assert.deepEqual(repositories.accounts.listByAdapter("qwen-web"), []);
  assert.deepEqual(
    repositories.accounts.findByAdapterAndExternalId("deepseek-web", "user-1"),
    linked,
  );
});

test("the auth-mode guard keeps api and account providers apart", () => {
  const account = repositories.accounts.create(accountDraft);
  const key = repositories.secrets.create({
    name: "Key",
    type: "custom",
    scope: "personal",
    createdAt: 100,
    updatedAt: 100,
  });

  assert.throws(
    () => repositories.providers.create(providerDraft),
    (error: unknown) => code(error) === AppErrorCode.VALIDATION_FAILED,
  );
  assert.throws(
    () =>
      repositories.providers.create({
        ...providerDraft,
        accountId: account.id,
        secretId: key.id,
      }),
    (error: unknown) => code(error) === AppErrorCode.VALIDATION_FAILED,
  );
  assert.throws(
    () =>
      repositories.providers.create({
        ...providerDraft,
        name: "Local",
        authMode: "api",
        adapter: "openai-compatible",
        accountId: account.id,
      }),
    (error: unknown) => code(error) === AppErrorCode.VALIDATION_FAILED,
  );

  const linked = repositories.providers.create({ ...providerDraft, accountId: account.id });
  assert.equal(linked.accountId, account.id);
  assert.equal(linked.secretId, null);
  assert.throws(
    () => repositories.providers.update(linked.id, { secretId: key.id }),
    (error: unknown) => code(error) === AppErrorCode.VALIDATION_FAILED,
  );
  assert.equal(repositories.providers.findById(linked.id)?.secretId, null);
});

test("removing an account detaches providers instead of deleting them", () => {
  const account = repositories.accounts.create(accountDraft);
  const linked = repositories.providers.create({
    ...providerDraft,
    accountId: account.id,
    settings: { temperature: 0.3, timeoutSeconds: 45 },
  });

  const removal = repositories.accounts.remove(account.id, 500);
  assert.deepEqual(removal.detachedProviderIds, [linked.id]);
  assert.equal(removal.freedSecretId, null);
  assert.equal(repositories.accounts.getById(account.id), undefined);

  const survivor = repositories.providers.findById(linked.id);
  assert.equal(survivor?.accountId, null);
  assert.equal(survivor?.status, "failed");
  assert.equal(survivor?.settings.timeoutSeconds, 45);
  assert.equal(survivor?.authMode, "account");
  assert.deepEqual(repositories.accounts.remove(account.id).detachedProviderIds, []);
});

test("both vendor payloads collapse onto one AccountIdentity", () => {
  const qwen = mapQwenIdentity(qwenPayload);
  assert.deepEqual(qwen.identity, {
    externalId: "qwen-9",
    emailMasked: "k***@mail.ru",
    displayName: "Кирилл",
    avatarUrl: "data:image/png;base64,AAAA",
    expiresAt: 4_102_444_800,
  });
  assert.deepEqual(qwen.credential, { token: "qwen-token-1", tokenType: "Bearer" });

  const deepseek = mapDeepSeekIdentity(deepSeekPayload);
  assert.deepEqual(deepseek.identity, {
    externalId: "4242",
    emailMasked: "k***@mail.ru",
    displayName: "Кирилл",
    avatarUrl: "https://cdn.deepseek.com/a.png",
  });
  assert.deepEqual(deepseek.credential, { token: "deepseek-token-1", tokenType: "Bearer" });
  assert.equal("expiresAt" in deepseek.identity, false);

  const sparse = mapQwenIdentity({ id: 7 });
  assert.deepEqual(sparse, { identity: { externalId: "7" }, credential: null });
  assert.throws(
    () => mapQwenIdentity({ email: "nobody" }),
    (error: unknown) => code(error) === AppErrorCode.VALIDATION_FAILED,
  );
  assert.deepEqual(Object.keys(IDENTITY_PROBES).sort(), ["deepseek-web", "qwen-web"]);
  assert.equal(identityProbe("qwen-web").endpoint, QWEN_IDENTITY_ENDPOINT);
  assert.equal(identityProbe("deepseek-web").endpoint, DEEPSEEK_IDENTITY_ENDPOINT);
});

test("a 200 with a non-zero DeepSeek code is a failure, not a signed-in user", () => {
  for (const payload of [
    { ...deepSeekPayload, code: 40_003 },
    { ...deepSeekPayload, data: { ...deepSeekPayload.data, biz_code: 1 } },
    { code: 0, data: { biz_code: 0, biz_data: null } },
  ]) {
    assert.throws(
      () => mapDeepSeekIdentity(payload),
      (error: unknown) => code(error) === AppErrorCode.PROVIDER_SESSION_EXPIRED,
    );
  }
});

test("a login redirect to HTML is a session expiry, not a parse error", async () => {
  const session = createFakeSession();
  session.queue(QWEN_IDENTITY_ENDPOINT, {
    status: 200,
    headers: { "content-type": "text/html; charset=utf-8" },
    text: "<!doctype html><title>Sign in</title>",
  });
  assert.equal(
    code(
      await rejection(() => identityProbe("qwen-web").probe(session, AbortSignal.timeout(1000))),
    ),
    AppErrorCode.PROVIDER_SESSION_EXPIRED,
  );

  session.reset();
  session.queue(DEEPSEEK_IDENTITY_ENDPOINT, { status: 401, body: { code: 40_100 } });
  assert.equal(
    code(
      await rejection(() =>
        identityProbe("deepseek-web").probe(session, AbortSignal.timeout(1000)),
      ),
    ),
    AppErrorCode.PROVIDER_SESSION_EXPIRED,
  );

  session.reset();
  session.queue(DEEPSEEK_IDENTITY_ENDPOINT, { error: new Error("getaddrinfo ENOTFOUND") });
  const unreachable = await rejection(() =>
    identityProbe("deepseek-web").probe(session, AbortSignal.timeout(1000)),
  );
  assert.equal(
    isAppError(unreachable) &&
      (unreachable.code === AppErrorCode.PROVIDER_UNREACHABLE ||
        unreachable.code === AppErrorCode.UNKNOWN),
    true,
  );
});

test("a captured token reaches SecretService and nothing else", async () => {
  const logger = capturingLogger();
  const session = createFakeSession();
  session.queue(QWEN_IDENTITY_ENDPOINT, { body: qwenPayload });
  const account = repositories.accounts.create({
    ...accountDraft,
    adapter: "qwen-web",
    externalId: "qwen-9",
  });
  const store = new AccountCredentialStore({
    accountId: account.id,
    accounts: repositories.accounts,
    secrets,
    session,
    clock: () => 1_000_000,
    logger,
  });

  const identity = await store.probe(AbortSignal.timeout(1000));
  assert.deepEqual(identity, {
    externalId: "qwen-9",
    emailMasked: "k***@mail.ru",
    displayName: "Кирилл",
    avatarUrl: "data:image/png;base64,AAAA",
    expiresAt: 4_102_444_800,
  });
  assert.equal("token" in identity, false);

  const stored = repositories.accounts.getById(account.id);
  assert.notEqual(stored?.tokenSecretId, null);
  assert.equal(stored?.tokenExpiresAt, 4_102_444_800);
  assert.equal(stored?.status, "linked");
  assert.equal(JSON.stringify(stored).includes("qwen-token-1"), false);
  assert.equal(logger.lines.join("\n").includes("qwen-token-1"), false);
  assert.equal(await secrets.resolve(stored!.tokenSecretId!), "qwen-token-1");
  assert.deepEqual(secrets.usage(stored!.tokenSecretId!), [{ kind: "account", id: account.id }]);

  const row = database.client.db.$client
    .prepare("SELECT * FROM account WHERE id = ?")
    .get(account.id);
  assert.equal(JSON.stringify(row).includes("qwen-token-1"), false);

  const secretId = stored!.tokenSecretId!;
  store.unlink();
  assert.equal(repositories.accounts.getById(account.id), undefined);
  assert.equal(
    secrets.list().some((entry) => entry.id === secretId),
    false,
  );
});

test("an expired token re-probes identity exactly once and then retries", async () => {
  const session = createFakeSession();
  session.queue(QWEN_IDENTITY_ENDPOINT, {
    body: { ...qwenPayload, token: "qwen-token-2", expires_at: 4_102_444_800 },
  });
  const account = repositories.accounts.create({
    ...accountDraft,
    adapter: "qwen-web",
    externalId: "qwen-9",
  });
  const stale = secrets.create({
    type: "account-token",
    name: "qwen-web:qwen-9",
    scope: "personal",
    value: "qwen-token-stale",
  });
  repositories.accounts.update(account.id, {
    tokenSecretId: stale.id,
    tokenExpiresAt: 1_000,
    updatedAt: 200,
  });
  const store = new AccountCredentialStore({
    accountId: account.id,
    accounts: repositories.accounts,
    secrets,
    session,
    clock: () => 2_000_000,
  });

  const token = await store.current(AbortSignal.timeout(1000));
  assert.deepEqual(token, { token: "qwen-token-2", tokenType: "Bearer" });
  assert.equal(session.requests.length, 1);
  assert.equal(await secrets.resolve(stale.id), "qwen-token-2");
  assert.equal(repositories.accounts.getById(account.id)?.tokenSecretId, stale.id);

  const chat = "https://chat.qwen.ai/api/v2/chat/completions";
  session.queue(chat, { status: 401, body: { detail: "expired" } }, { body: { ok: true } });
  const transport = new AccountTransport({
    baseUrl: "https://chat.qwen.ai/api/v2",
    session,
    credentials: store,
  });
  const response = await transport.request(
    { method: "POST", path: "chat/completions", body: { model: "qwen3" } },
    AbortSignal.timeout(1000),
  );

  assert.equal(response.status, 200);
  assert.deepEqual(response.json(), { ok: true });
  assert.equal(session.requests.filter((entry) => entry.url === chat).length, 2);
  assert.equal(session.requests.filter((entry) => entry.url === QWEN_IDENTITY_ENDPOINT).length, 2);
  const chatCalls = session.requests.filter((entry) => entry.url === chat);
  assert.equal(chatCalls[0]?.headers.authorization, "Bearer qwen-token-2");
  assert.equal(chatCalls[1]?.headers.authorization, "Bearer qwen-token-2");
  assert.equal(chatCalls[0]?.headers["user-agent"], FAKE_USER_AGENT);
  assert.equal(chatCalls[0]?.headers.origin, "https://chat.qwen.ai");
  assert.equal(chatCalls[0]?.headers.referer, "https://chat.qwen.ai/");
  assert.equal("cookie" in chatCalls[0]!.headers, false);
});

test("a session that stays dead ends as PROVIDER_SESSION_EXPIRED after one re-probe", async () => {
  const session = createFakeSession();
  const account = repositories.accounts.create({
    ...accountDraft,
    adapter: "qwen-web",
    externalId: "qwen-9",
  });
  const stale = secrets.create({
    type: "account-token",
    name: "qwen-web:qwen-9",
    scope: "personal",
    value: "qwen-token-stale",
  });
  repositories.accounts.update(account.id, { tokenSecretId: stale.id, updatedAt: 200 });
  const store = new AccountCredentialStore({
    accountId: account.id,
    accounts: repositories.accounts,
    secrets,
    session,
    clock: () => 2_000_000,
  });

  const chat = "https://chat.qwen.ai/api/v2/chat/completions";
  session.queue(chat, { status: 401, body: { detail: "expired" } });
  session.queue(QWEN_IDENTITY_ENDPOINT, {
    status: 200,
    headers: { "content-type": "text/html" },
    text: "<!doctype html>",
  });
  const transport = new AccountTransport({
    baseUrl: "https://chat.qwen.ai/api/v2",
    session,
    credentials: store,
  });

  assert.equal(
    code(
      await rejection(() =>
        transport.request({ method: "GET", path: "chat/completions" }, AbortSignal.timeout(1000)),
      ),
    ),
    AppErrorCode.PROVIDER_SESSION_EXPIRED,
  );
  assert.equal(session.requests.filter((entry) => entry.url === QWEN_IDENTITY_ENDPOINT).length, 1);
  assert.equal(session.requests.filter((entry) => entry.url === chat).length, 1);
  assert.equal(repositories.accounts.getById(account.id)?.status, "needs-relink");
});

test("an account-mode provider without a live account never reaches the network", async () => {
  const session = createFakeSession();
  const account = repositories.accounts.create(accountDraft);
  const linked = repositories.providers.create({ ...providerDraft, accountId: account.id });
  const registry = new ProviderRegistry({
    providers: repositories.providers,
    secrets,
    accounts: repositories.accounts,
    sessions: () => session,
  });

  assert.notEqual(
    (await registry.driver(linked.id)).text,
    null,
    "a linked account must get as far as a built adapter",
  );

  repositories.accounts.updateStatus(account.id, "needs-relink", "Куки очищены", 900);
  assert.equal(
    code(await rejection(() => registry.driver(linked.id))),
    AppErrorCode.PROVIDER_SESSION_EXPIRED,
  );

  repositories.accounts.remove(account.id, 1000);
  const detached = repositories.providers.findById(linked.id)!;
  assert.equal(detached.accountId, null);
  assert.equal(
    code(await rejection(() => registry.driver(linked.id))),
    AppErrorCode.PROVIDER_SESSION_EXPIRED,
  );
  assert.equal(session.requests.length, 0);

  const relinked = repositories.accounts.create({ ...accountDraft, externalId: "user-2" });
  repositories.providers.update(detached.id, { accountId: relinked.id, status: "unknown" });
  assert.notEqual((await registry.driver(detached.id)).text, null);

  const noSessions = new ProviderRegistry({
    providers: repositories.providers,
    secrets,
    accounts: repositories.accounts,
  });
  assert.equal(
    code(await rejection(() => noSessions.driver(detached.id))),
    AppErrorCode.VALIDATION_FAILED,
  );

  const apiOnly = repositories.providers.create({
    ...providerDraft,
    name: "Local",
    adapter: "openai-compatible",
    authMode: "api",
    baseUrl: "http://localhost:11434/v1",
  });
  const bare = new ProviderRegistry({ providers: repositories.providers, secrets });
  assert.notEqual(await bare.driver(apiOnly.id), undefined);
  assert.equal(session.requests.length, 0);
});

test("an api-mode 401 stays PROVIDER_AUTH_FAILED while an account-mode 401 does not", async () => {
  const session = createFakeSession();
  const url = "https://chat.deepseek.com/api/v0/chat";
  session.queue(url, { status: 401, body: { code: 40_100 } });
  const transport = new AccountTransport({ baseUrl: "https://chat.deepseek.com/api/v0", session });
  assert.equal(
    code(
      await rejection(() =>
        transport.request({ method: "GET", path: "chat" }, AbortSignal.timeout(1000)),
      ),
    ),
    AppErrorCode.PROVIDER_SESSION_EXPIRED,
  );

  const provider = repositories.providers.create({
    ...providerDraft,
    name: "Local",
    adapter: "openai-compatible",
    authMode: "api",
    baseUrl: "http://localhost:11434/v1",
  });
  const registry = new ProviderRegistry({
    providers: repositories.providers,
    secrets,
    fetch: async () =>
      await Promise.resolve(
        new Response(JSON.stringify({ error: "bad key" }), {
          status: 401,
          headers: { "content-type": "application/json" },
        }),
      ),
  });
  const text = await registry.text(provider.id);
  assert.equal(
    code(
      await rejection(() =>
        text.generate(
          { model: "llama3", messages: [{ role: "user", content: "Привет" }] },
          AbortSignal.timeout(1000),
        ),
      ),
    ),
    AppErrorCode.PROVIDER_AUTH_FAILED,
  );
});

test("no test imports electron, so cookies stay inside the partition", () => {
  const roots = [HERE, join(HERE, "renderer"), join(HERE, "..", "..", "..", "test", "helpers")];
  const files = roots.flatMap((root) =>
    readdirSync(root, { withFileTypes: true })
      .filter((entry) => entry.isFile() && /\.tsx?$/.test(entry.name))
      .map((entry) => join(root, entry.name)),
  );
  assert.equal(files.length > 0, true);
  for (const file of files) {
    const source = readFileSync(file, "utf8");
    assert.equal(/from ["']electron["']/.test(source), false, `${file} imports electron`);
    assert.equal(/cookies\s*\./.test(source), false, `${file} touches a cookies API`);
  }
});
