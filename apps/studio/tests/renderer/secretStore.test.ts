import assert from "node:assert/strict";
import { test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppError,
  AppErrorCode,
  contract,
  SECRET_TYPE_REGISTRY,
  type Contract,
  type SecretDto,
  type SecretId,
  type SecretSummaryDto,
} from "@zvs/shared";
import { SecretStore } from "../../src/renderer/features/secrets/SecretStore.ts";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";

const OLLAMA = "0199aa11-1111-7111-8111-000000000001" as SecretId;
const OPENROUTER = "0199aa11-1111-7111-8111-000000000002" as SecretId;
const CREATED = "0199aa11-1111-7111-8111-000000000003" as SecretId;

function summary(overrides: Partial<SecretSummaryDto> & { id: SecretId }): SecretSummaryDto {
  return {
    type: "ollama-cloud",
    name: "Ollama Cloud — личный",
    scope: "personal",
    hint: "osk_live_••••4f2a",
    tags: ["llm"],
    usageCount: 2,
    rotationStatus: "ok",
    rotatesAt: null,
    updatedAt: 1_700_000_000_000,
    ...overrides,
  };
}

function detail(base: SecretSummaryDto): SecretDto {
  return { ...base, fields: { baseUrl: "https://ollama.com/api", verifyTls: true }, note: null };
}

function storeWith(summaries: SecretSummaryDto[]) {
  const bridge = createFakeBridge<Contract>();
  bridge.handle("secrets.types", () => SECRET_TYPE_REGISTRY.map((schema) => ({ ...schema })));
  bridge.handle("secrets.list", () => summaries);
  bridge.handle("secrets.get", ({ id }) => {
    const found = summaries.find((secret) => secret.id === id);
    if (found === undefined) throw new AppError(AppErrorCode.NOT_FOUND, "Секрет не найден");
    return detail(found);
  });
  return { bridge, store: new SecretStore(createIpcClient(contract, bridge)) };
}

test("load fetches the registry and the list once", async () => {
  const rows = [
    summary({ id: OLLAMA }),
    summary({ id: OPENROUTER, type: "openrouter", name: "OpenRouter", scope: "shared" }),
  ];
  const { bridge, store } = storeWith(rows);

  await store.load();

  assert.equal(store.loaded, true);
  assert.equal(store.loading, false);
  assert.equal(store.types.length, SECRET_TYPE_REGISTRY.length);
  assert.equal(store.total, 2);
  assert.deepEqual(store.counts, { all: 2, personal: 1, shared: 1, public: 0 });

  await store.load();
  assert.equal(bridge.calls.filter((call) => call.channel === "secrets.types").length, 1);
});

test("a failing host leaves the page usable and shows mapped copy", async () => {
  const bridge = createFakeBridge<Contract>();
  bridge.fail("secrets.types", AppErrorCode.DB_ERROR, "raw host text");
  const store = new SecretStore(createIpcClient(contract, bridge));

  await store.load();

  assert.equal(store.loaded, true);
  assert.equal(store.error, "Сбой локальной базы данных.");
  assert.notEqual(store.error, "raw host text");
});

test("filter and query narrow the list while counts follow the query", async () => {
  const { store } = storeWith([
    summary({ id: OLLAMA }),
    summary({
      id: OPENROUTER,
      type: "openrouter",
      name: "OpenRouter — main",
      scope: "shared",
      tags: ["cloud"],
    }),
  ]);
  await store.load();

  store.setFilter("shared");
  assert.deepEqual(
    store.visible.map((secret) => secret.id),
    [OPENROUTER],
  );

  store.setFilter("all");
  store.setQuery("main");
  assert.deepEqual(
    store.visible.map((secret) => secret.id),
    [OPENROUTER],
  );
  assert.deepEqual(store.counts, { all: 1, personal: 0, shared: 1, public: 0 });
});

test("selecting a secret loads its detail and builds a clean form", async () => {
  const { store } = storeWith([summary({ id: OLLAMA })]);
  await store.load();

  await store.select(OLLAMA);

  assert.equal(store.selectedId, OLLAMA);
  assert.notEqual(store.form, null);
  assert.equal(store.form?.name, "Ollama Cloud — личный");
  assert.equal(store.form?.dirty, false);
  assert.equal(store.form?.isNew, false);
});

test("a dirty form defers the next selection until the discard is confirmed", async () => {
  const rows = [
    summary({ id: OLLAMA }),
    summary({ id: OPENROUTER, type: "openrouter", name: "OpenRouter" }),
  ];
  const { store } = storeWith(rows);
  await store.load();
  await store.select(OLLAMA);
  store.form?.setName("Изменено");

  store.requestSelect(OPENROUTER);
  assert.deepEqual(store.pending, { kind: "select", id: OPENROUTER });
  assert.equal(store.selectedId, OLLAMA);

  store.confirmPending();
  await Promise.resolve();
  await Promise.resolve();

  assert.equal(store.pending, null);
  assert.equal(store.selectedId, OPENROUTER);
});

test("create adds the secret to the list and selects it", async () => {
  const rows = [summary({ id: OLLAMA })];
  const { bridge, store } = storeWith(rows);
  bridge.handle("secrets.create", (input) =>
    detail(summary({ id: CREATED, name: input.name, type: input.type, scope: input.scope })),
  );
  await store.load();
  store.startCreate();
  store.form?.setName("Новый ключ");
  store.form?.setValue("osk_live_value");

  const saved = await store.submit();

  assert.equal(saved, true);
  assert.equal(store.total, 2);
  assert.equal(store.selectedId, CREATED);
  assert.equal(store.form?.dirty, false);
  const call = bridge.calls.find((entry) => entry.channel === "secrets.create");
  assert.equal((call?.payload as { name: string }).name, "Новый ключ");
});

test("a validation failure from the host lands on the field, not in a banner", async () => {
  const { bridge, store } = storeWith([summary({ id: OLLAMA })]);
  bridge.handle("secrets.create", () => {
    throw new AppError(AppErrorCode.VALIDATION_FAILED, "raw host text", {
      details: {
        field: "fields",
        type: "ollama-cloud",
        issues: [{ path: "baseUrl", code: "invalid_format" }],
      },
    });
  });
  await store.load();
  store.startCreate();
  store.form?.setName("Ключ");
  store.form?.setValue("osk_live_value");

  const saved = await store.submit();

  assert.equal(saved, false);
  assert.equal(store.form?.errorOf("fields.baseUrl"), "Некорректный формат.");
  assert.equal(store.form?.banner, null);
});

test("deleting a used secret raises the conflict dialog with its consumers", async () => {
  const { bridge, store } = storeWith([summary({ id: OLLAMA })]);
  bridge.handle("secrets.remove", () => {
    throw new AppError(AppErrorCode.CONFLICT, "raw host text", {
      details: { secretId: OLLAMA, consumers: [{ kind: "provider", id: "provider-1" }] },
    });
  });
  await store.load();
  await store.select(OLLAMA);

  const removed = await store.removeSelected();

  assert.equal(removed, false);
  assert.equal(store.total, 1);
  assert.equal(store.error, null);
  assert.deepEqual(store.conflict?.consumers, [{ kind: "provider", id: "provider-1" }]);

  store.dismissConflict();
  assert.equal(store.conflict, null);
});

test("deleting an unused secret drops it from the list and clears the form", async () => {
  const { bridge, store } = storeWith([summary({ id: OLLAMA })]);
  bridge.handle("secrets.remove", ({ id }) => ({ id, removed: true as const }));
  await store.load();
  await store.select(OLLAMA);

  const removed = await store.removeSelected();

  assert.equal(removed, true);
  assert.equal(store.total, 0);
  assert.equal(store.form, null);
  assert.equal(store.selectedId, null);
});
