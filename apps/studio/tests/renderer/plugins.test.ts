import assert from "node:assert/strict";
import { test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppError,
  AppErrorCode,
  contract,
  type Contract,
  type FieldDescriptor,
  type IntegrationAccountDetailDto,
  type IntegrationAccountDto,
  type IntegrationConfig,
  type IntegrationId,
  type IntegrationKind,
  type IntegrationKindDto,
  type SecretId,
  type SecretSummaryDto,
  type StreamId,
} from "@zvs/shared";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import PluginStore from "../../src/renderer/features/plugins/PluginStore.ts";
import { PluginFormVm, secretOptions } from "../../src/renderer/features/plugins/PluginFormVm.ts";
import { SIGN_IN_EXPIRED } from "../../src/renderer/features/plugins/pluginErrors.ts";
import {
  accountsWord,
  defaultAccountName,
  formatCountdown,
} from "../../src/renderer/features/plugins/pluginPresentation.ts";
import { SIGN_IN_TYPES, signInTypeOf } from "../../src/renderer/features/plugins/signInTypes.ts";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createFakeClock } from "../../../../test/helpers/fakeClock.ts";

const STREAM = "0199cc22-5555-7111-8111-000000000001" as StreamId;
const SECRET_A = "0199cc22-6666-7111-8111-000000000001" as SecretId;
const SECRET_B = "0199cc22-6666-7111-8111-000000000002" as SecretId;

function id(index: number): IntegrationId {
  return `0199cc22-4444-7111-8111-${String(index).padStart(12, "0")}` as IntegrationId;
}

function field(
  key: string,
  kind: FieldDescriptor["kind"],
  extra: Partial<FieldDescriptor> = {},
): FieldDescriptor {
  return { key, label: key, kind, required: false, ...extra };
}

const MAIL_FIELDS: FieldDescriptor[] = [
  field("protocol", "select", {
    options: [
      { value: "imap", label: "IMAP / SMTP" },
      { value: "google", label: "Google" },
    ],
    default: "imap",
  }),
  field("access", "select", { options: [{ value: "read", label: "Только чтение" }] }),
  field("address", "text", { required: true }),
  field("imapHost", "text"),
  field("imapPort", "number", { default: 993 }),
  field("imapUser", "text"),
  field("imapPassword", "secret"),
  field("smtpReuseLogin", "boolean", { default: true }),
  field("smtpUser", "text"),
  field("smtpPassword", "secret"),
  field("oauthClientId", "text"),
  field("oauthClientSecret", "secret"),
  field("oauthToken", "secret"),
];

const GIT_FIELDS: FieldDescriptor[] = [
  field("auth", "select", {
    options: [
      { value: "token", label: "Токен доступа" },
      { value: "oauth", label: "Вход через браузер" },
    ],
    default: "token",
  }),
  field("token", "secret"),
  field("baseUrl", "url", { default: "https://api.github.com" }),
  field("oauthClientId", "text", { help: "Client ID приложения" }),
  field("oauthToken", "secret"),
  field("repositories", "text"),
];

const TELEGRAM_FIELDS: FieldDescriptor[] = [field("botToken", "secret", { required: true })];

function kind(
  value: IntegrationKind,
  fields: FieldDescriptor[],
  sideEffect = false,
): IntegrationKindDto {
  return {
    kind: value,
    label: value,
    fields,
    triggers: [{ name: `${value}.event`, label: "Событие", description: "" }],
    tools: [{ name: `${value}.send`, label: "Отправить", description: "", sideEffect }],
  };
}

const KINDS = [
  kind("mail", MAIL_FIELDS, true),
  kind("telegram", TELEGRAM_FIELDS),
  kind("github", GIT_FIELDS),
  kind("gitlab", GIT_FIELDS),
];

function account(
  index: number,
  kindValue: IntegrationKind,
  config: IntegrationConfig = {},
  overrides: Partial<IntegrationAccountDto> = {},
): IntegrationAccountDto {
  return {
    id: id(index),
    kind: kindValue,
    name: `${kindValue} ${String(index)}`,
    enabled: true,
    config,
    status: "running",
    statusDetail: null,
    lastActivityAt: null,
    counters: { events: 0, duplicates: 0, runs: 0, toolCalls: 0, errors: 0 },
    createdAt: 1_700_000_000_000 + index,
    updatedAt: 1_700_000_000_000 + index,
    ...overrides,
  };
}

function secret(secretId: SecretId, name: string): SecretSummaryDto {
  return {
    id: secretId,
    name,
    scope: "personal",
    hint: null,
    tags: [],
    usageCount: 0,
    rotationStatus: "ok",
    rotatesAt: null,
    updatedAt: 1_700_000_000_000,
  };
}

function detail(entry: IntegrationAccountDto): IntegrationAccountDetailDto {
  return { ...entry, triggers: [], tools: [] };
}

function storeWith(accounts: IntegrationAccountDto[]) {
  const bridge = createFakeBridge<Contract>();
  const events = createEventRouter();
  const clock = createFakeClock();
  let listed = [...accounts];
  const find = (input: { id: IntegrationId }): IntegrationAccountDto => {
    const found = listed.find((entry) => entry.id === input.id);
    if (found === undefined) throw new AppError(AppErrorCode.NOT_FOUND, "missing");
    return found;
  };
  const replace = (next: IntegrationAccountDto): IntegrationAccountDto => {
    listed = listed.map((entry) => (entry.id === next.id ? next : entry));
    return next;
  };
  bridge.handle("integrations.kinds", () => KINDS);
  bridge.handle("integrations.list", () => [...listed]);
  bridge.handle("integrations.get", (input) => detail(find(input)));
  bridge.handle("integrations.activity", () => []);
  bridge.handle("integrations.enable", (input) =>
    replace({ ...find(input), enabled: true, status: "running" }),
  );
  bridge.handle("integrations.disable", (input) =>
    replace({ ...find(input), enabled: false, status: "disabled" }),
  );
  bridge.handle("integrations.remove", (input) => {
    listed = listed.filter((entry) => entry.id !== input.id);
    return { id: input.id, removed: true as const };
  });
  bridge.handle("secrets.list", () => [secret(SECRET_A, "Пароль"), secret(SECRET_B, "Токен")]);
  bridge.handle("scenarios.list", () => []);
  bridge.handle("workers.list", () => []);
  const store = new PluginStore(createIpcClient(contract, bridge), events, { clock });
  return { bridge, events, clock, store, listed: () => listed };
}

test("accounts are grouped by kind, several per kind and per sign-in type", async () => {
  const { store } = storeWith([
    account(1, "github", { auth: "token", token: { secretId: SECRET_A } }),
    account(2, "github", { auth: "oauth", oauthToken: { secretId: SECRET_B } }),
    account(3, "mail", { protocol: "google" }),
    account(4, "mail", { protocol: "imap" }),
    account(5, "mail", { protocol: "imap" }),
    account(6, "telegram"),
  ]);
  await store.load();

  const github = store.accountsOf("github");
  assert.deepEqual(
    github.map((entry) => signInTypeOf(entry).id),
    ["githubToken", "githubOauth"],
  );
  assert.deepEqual(
    store.accountsOf("mail").map((entry) => signInTypeOf(entry).id),
    ["google", "imap", "imap"],
  );
  assert.equal(store.accountsOf("gitlab").length, 0);
  assert.equal(accountsWord(store.accountsOf("mail").length), "3 аккаунта");
});

test("the catalogue filters by tab and searches name and description", async () => {
  const { store } = storeWith([account(1, "telegram")]);
  await store.load();

  assert.deepEqual(
    store.sections.map((section) => section.entries.map((entry) => entry.kind)),
    [
      ["mail", "telegram"],
      ["github", "gitlab"],
    ],
  );

  store.setTab("connected");
  assert.deepEqual(
    store.sections.map((section) => section.entries.map((entry) => [entry.kind, entry.count])),
    [[["telegram", 1]]],
  );

  store.setTab("all");
  store.setQuery("слияни");
  assert.deepEqual(
    store.sections.flatMap((section) => section.entries.map((entry) => entry.kind)),
    ["github", "gitlab"],
  );

  store.setQuery("нет такого");
  assert.equal(store.catalogueEmpty, true);
});

test("a plugin with accounts opens its first account, one without opens the add flow", async () => {
  const { store } = storeWith([account(1, "github", { auth: "token" })]);
  await store.load();

  store.select("github", null);
  assert.equal(store.consumeRedirect(), `/plugins/github/${id(1)}`);

  store.select("telegram", null);
  assert.equal(store.consumeRedirect(), null);
  assert.equal(store.form?.isNew, true);
  assert.equal(store.form?.type.id, "telegramBot");

  store.select("mail", null);
  assert.equal(store.form === null, true);
  store.chooseType("google");
  assert.deepEqual(store.form?.config(), { protocol: "google" });
});

test("the device-code step fills the sign-in panel and cancel clears it", async () => {
  const { bridge, events, store } = storeWith([
    account(1, "github", { auth: "oauth" }, { enabled: false, status: "disabled" }),
  ]);
  await store.mount();
  store.select("github", id(1));

  let finish: (error: AppError) => void = () => undefined;
  bridge.handle(
    "integrations.signIn",
    () =>
      new Promise((_, reject) => {
        finish = reject;
      }),
  );
  bridge.handle("integrations.cancelSignIn", () => {
    finish(new AppError(AppErrorCode.CONFLICT, "Вход отменён"));
    return account(1, "github", { auth: "oauth" });
  });

  const running = store.startSignIn();
  assert.equal(store.stateOf(store.accountsOf("github")[0] as IntegrationAccountDto), "signing");

  events.dispatch({
    streamId: STREAM,
    seq: 0,
    ts: 1_700_000_000_000,
    type: "step",
    step: {
      domain: "integrations",
      accountId: id(2),
      phase: "device-code",
      userCode: "XXXX-0000",
    },
  });
  assert.equal(store.signIn?.userCode, null);

  events.dispatch({
    streamId: STREAM,
    seq: 1,
    ts: 1_700_000_000_000,
    type: "step",
    step: {
      domain: "integrations",
      accountId: id(1),
      phase: "device-code",
      userCode: "WDJB-MJHT",
      verificationUri: "https://github.com/login/device",
      expiresAt: 1_700_000_900_000,
    },
  });
  assert.deepEqual(store.signIn, {
    accountId: id(1),
    userCode: "WDJB-MJHT",
    verificationUri: "https://github.com/login/device",
    expiresAt: 1_700_000_900_000,
  });
  assert.equal(formatCountdown((store.signIn?.expiresAt ?? 0) - store.now()), "15:00");

  await store.cancelSignIn();
  await running;
  assert.equal(store.signIn, null);
  assert.equal(store.error, null);
});

test("a successful browser sign-in on a new account enables it", async () => {
  const { bridge, store, listed } = storeWith([]);
  await store.load();
  store.select("github", null);
  store.chooseType("githubOauth");
  bridge.handle("integrations.create", (input) => {
    const created = account(9, input.kind, input.config ?? {}, {
      name: input.name,
      enabled: input.enabled ?? true,
      status: input.enabled === false ? "disabled" : "running",
    });
    bridge.handle("integrations.list", () => [...listed(), created]);
    return created;
  });
  bridge.handle("integrations.signIn", () =>
    account(
      9,
      "github",
      { auth: "oauth", oauthToken: { secretId: SECRET_A } },
      {
        name: "GitHub",
        enabled: false,
        status: "disabled",
      },
    ),
  );
  bridge.handle("integrations.enable", () =>
    account(9, "github", { auth: "oauth", oauthToken: { secretId: SECRET_A } }, { name: "GitHub" }),
  );
  bridge.handle("integrations.get", () =>
    detail(
      account(
        9,
        "github",
        { auth: "oauth", oauthToken: { secretId: SECRET_A } },
        { name: "GitHub" },
      ),
    ),
  );

  await store.startSignIn();

  const create = bridge.calls.find((call) => call.channel === "integrations.create");
  assert.deepEqual(create?.payload, {
    kind: "github",
    name: "GitHub",
    config: { baseUrl: "https://api.github.com", auth: "oauth" },
    enabled: false,
  });
  assert.equal(store.consumeRedirect(), `/plugins/github/${id(9)}`);
  assert.ok(bridge.calls.some((call) => call.channel === "integrations.enable"));
  const saved = store.accountsOf("github")[0] as IntegrationAccountDto;
  assert.equal(saved.enabled, true);
  assert.equal(store.stateOf(saved), "running");
});

test("an expired session marks the account as needing sign-in", async () => {
  const { bridge, store } = storeWith([
    account(1, "gitlab", { auth: "oauth", oauthToken: { secretId: SECRET_A } }),
  ]);
  await store.load();
  store.select("gitlab", id(1));
  bridge.fail("integrations.test", AppErrorCode.PROVIDER_SESSION_EXPIRED, "raw");

  await store.test();

  const entry = store.accountsOf("gitlab")[0] as IntegrationAccountDto;
  assert.equal(store.stateOf(entry), "needsSignIn");
  assert.equal(store.error, SIGN_IN_EXPIRED);
});

test("enable, disable and remove round-trip through the host", async () => {
  const { bridge, store } = storeWith([account(1, "telegram"), account(2, "telegram")]);
  await store.load();
  store.select("telegram", id(1));

  await store.setEnabled(false);
  let entry = store.accountsOf("telegram")[0] as IntegrationAccountDto;
  assert.equal(entry.enabled, false);
  assert.equal(store.stateOf(entry), "disabled");

  await store.setEnabled(true);
  entry = store.accountsOf("telegram")[0] as IntegrationAccountDto;
  assert.equal(store.stateOf(entry), "running");

  await store.remove();
  assert.deepEqual(
    store.accountsOf("telegram").map((remaining) => remaining.id),
    [id(2)],
  );
  assert.equal(store.consumeRedirect(), `/plugins/telegram/${id(2)}`);
  assert.deepEqual(
    bridge.calls
      .map((call) => call.channel)
      .filter((channel) => /enable|disable|remove/.test(channel)),
    ["integrations.disable", "integrations.enable", "integrations.remove"],
  );
});

test("each sign-in type hides the fields that belong to the others", () => {
  const visible = (typeId: string, fields: FieldDescriptor[]) => {
    const type = Object.values(SIGN_IN_TYPES)
      .flat()
      .find((entry) => entry.id === typeId);
    assert.ok(type);
    return new PluginFormVm(fields, type, null, "Аккаунт").visibleFields.map((entry) => entry.key);
  };

  assert.deepEqual(visible("google", MAIL_FIELDS), [
    "access",
    "address",
    "oauthClientId",
    "oauthClientSecret",
  ]);
  assert.deepEqual(visible("imap", MAIL_FIELDS), [
    "access",
    "address",
    "imapHost",
    "imapPort",
    "imapUser",
    "imapPassword",
    "smtpReuseLogin",
  ]);
  assert.deepEqual(visible("githubToken", GIT_FIELDS), ["token", "baseUrl", "repositories"]);
  assert.deepEqual(visible("githubOauth", GIT_FIELDS), [
    "baseUrl",
    "oauthClientId",
    "repositories",
  ]);
});

test("the form sends the fixed choice, secret references and keeps hidden stored values", () => {
  const type = SIGN_IN_TYPES.mail.find((entry) => entry.id === "imap");
  assert.ok(type);
  const vm = new PluginFormVm(
    MAIL_FIELDS,
    type,
    account(1, "mail", { protocol: "imap", oauthToken: { secretId: SECRET_B } }),
    "mail 1",
  );

  assert.equal(vm.validate(), false);
  assert.ok(vm.errorOf("config.address"));
  assert.ok(vm.errorOf("config.imapPassword"));

  vm.setValue("address", "support@example.com");
  vm.setValue("imapHost", "imap.example.com");
  vm.setValue("imapUser", "support");
  vm.setValue("imapPassword", SECRET_A);
  vm.setValue("smtpReuseLogin", false);
  assert.ok(vm.visibleFields.some((entry) => entry.key === "smtpPassword"));
  assert.equal(vm.validate(), true);
  assert.deepEqual(vm.config(), {
    oauthToken: { secretId: SECRET_B },
    address: "support@example.com",
    imapHost: "imap.example.com",
    imapPort: 993,
    imapUser: "support",
    imapPassword: { secretId: SECRET_A },
    smtpReuseLogin: false,
    protocol: "imap",
  });
});

test("the secret picker lists every secret", async () => {
  const { store } = storeWith([]);
  await store.load();

  assert.deepEqual(
    store.secretChoices.map((option) => option.value),
    [SECRET_A, SECRET_B],
  );
  assert.deepEqual(
    secretOptions([secret(SECRET_B, "Б"), secret(SECRET_A, "А")]).map((option) => option.label),
    ["А", "Б"],
  );
});

test("a default name adds a number when the plugin label is taken", () => {
  assert.equal(defaultAccountName("github", []), "GitHub");
  assert.equal(defaultAccountName("github", [{ name: "GitHub" }]), "GitHub 2");
  assert.equal(
    defaultAccountName("github", [{ name: "github" }, { name: "GitHub 2" }]),
    "GitHub 3",
  );
});
