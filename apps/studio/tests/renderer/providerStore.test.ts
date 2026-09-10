import assert from "node:assert/strict";
import { test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppError,
  AppErrorCode,
  contract,
  type Contract,
  type ProbeResultDto,
  type ProviderCapability,
  type ProviderDto,
} from "@zvs/shared";
import { ProviderStore } from "../../src/renderer/features/providers/ProviderStore.ts";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import {
  ACCOUNTS,
  ADAPTERS,
  EMBEDDER,
  MODEL,
  OLLAMA,
  provider,
  SECRET,
  SECRETS,
  summary,
} from "./providerFixtures.ts";

const TEXT_ROW = summary({ id: OLLAMA });
const EMBEDDING_ROW = summary({
  id: EMBEDDER,
  name: "Ollama Embeddings",
  capabilities: ["embedding"],
  modelCount: 1,
});

function storeWith(options: { readonly detail?: ProviderDto } = {}) {
  const bridge = createFakeBridge<Contract>();
  const detail = options.detail ?? provider();
  bridge.handle("providers.adapters", () => ADAPTERS.map((entry) => ({ ...entry })));
  bridge.handle("secrets.list", () => SECRETS.map((entry) => ({ ...entry })));
  bridge.handle("accounts.list", () => [...ACCOUNTS]);
  bridge.handle("providers.list", ({ capability }) => {
    if (capability === "text") return [TEXT_ROW];
    if (capability === "embedding") return [EMBEDDING_ROW];
    return [];
  });
  bridge.handle("providers.get", ({ id }) => {
    if (id === OLLAMA) return detail;
    if (id === EMBEDDER) {
      return provider({ ...EMBEDDING_ROW, models: [], capabilities: ["embedding"] });
    }
    throw new AppError(AppErrorCode.NOT_FOUND, "Провайдер не найден");
  });
  return { bridge, store: new ProviderStore(createIpcClient(contract, bridge)) };
}

function listedCapabilities(
  bridge: ReturnType<typeof createFakeBridge<Contract>>,
): ProviderCapability[] {
  return bridge.calls
    .filter((call) => call.channel === "providers.list")
    .map((call) => (call.payload as { capability: ProviderCapability }).capability);
}

test("load fills every tab count and selects the first row of the active one", async () => {
  const { bridge, store } = storeWith();

  await store.load();

  assert.equal(store.loaded, true);
  assert.deepEqual(store.counts, { text: 1, embedding: 1, image: 0 });
  assert.deepEqual(
    store.summaries.map((row) => row.id),
    [OLLAMA],
  );
  assert.equal(store.selectedId, OLLAMA);
  assert.equal(store.form?.isNew, false);
  assert.deepEqual(listedCapabilities(bridge), ["text", "embedding", "image"]);
});

test("switching tabs refetches with the capability filter instead of filtering locally", async () => {
  const { bridge, store } = storeWith();
  await store.load();
  const before = bridge.calls.filter((call) => call.channel === "providers.list").length;

  await store.setCapability("embedding");

  assert.equal(store.capability, "embedding");
  assert.deepEqual(
    store.summaries.map((row) => row.id),
    [EMBEDDER],
  );
  assert.equal(bridge.calls.filter((call) => call.channel === "providers.list").length, before + 3);
  assert.equal(store.counts.text, 1);
});

test("a dirty form holds the tab switch until the user answers", async () => {
  const { store } = storeWith();
  await store.load();
  store.form?.setName("Изменено");

  store.requestCapability("embedding");

  assert.equal(store.capability, "text");
  assert.deepEqual(store.pending, { kind: "capability", capability: "embedding" });

  store.confirmPending();
  await Promise.resolve();
  assert.equal(store.pending, null);
});

test("probing a saved provider goes by id and refreshes status and models", async () => {
  const probed = provider({ status: "ok", lastLatencyMs: 412 });
  const result: ProbeResultDto = {
    providerId: OLLAMA,
    outcome: { kind: "ok", latencyMs: 412, live: true, models: [] },
    status: "ok",
    statusDetail: null,
    checkedAt: 1_700_000_100_000,
    provider: probed,
  };
  const { bridge, store } = storeWith();
  bridge.handle("providers.probe", (request) => {
    assert.deepEqual(request, { id: OLLAMA });
    return result;
  });
  await store.load();

  await store.probe();

  assert.equal(store.probing, false);
  assert.equal(store.probeResult?.outcome.kind, "ok");
  assert.equal(store.selectedSummary?.status, "ok");
  assert.equal(store.rows.length, 2);
});

test("probing an edited connection sends the draft, not the stored row", async () => {
  const { bridge, store } = storeWith();
  const seen: unknown[] = [];
  bridge.handle("providers.probe", (request) => {
    seen.push(request);
    return {
      providerId: null,
      outcome: { kind: "auth-failed" as const },
      status: "failed" as const,
      statusDetail: "Ключ отклонён — исправьте учётные данные",
      checkedAt: 1_700_000_100_000,
      provider: null,
    };
  });
  await store.load();
  store.form?.setBaseUrl("https://ollama.com/api/v2");

  await store.probe();

  assert.equal(seen.length, 1);
  assert.deepEqual(
    (seen[0] as { draft: { baseUrl: string } }).draft.baseUrl,
    "https://ollama.com/api/v2",
  );
  assert.equal(store.probeResult?.outcome.kind, "auth-failed");
});

test("session-expired is kept distinct from a rejected key", async () => {
  const { bridge, store } = storeWith();
  bridge.handle("providers.probe", () => ({
    providerId: OLLAMA,
    outcome: { kind: "session-expired" as const },
    status: "needs-relink" as const,
    statusDetail: "Сессия истекла — войдите заново",
    checkedAt: 1_700_000_100_000,
    provider: null,
  }));
  await store.load();

  await store.probe();

  assert.equal(store.probeResult?.outcome.kind, "session-expired");
  assert.equal(store.error, null);
});

test("choosing a default model persists it through the host", async () => {
  const { bridge, store } = storeWith();
  const calls: unknown[] = [];
  bridge.handle("providers.setDefaultModel", (input) => {
    calls.push(input);
    return provider({ defaultModelId: input.modelId });
  });
  await store.load();

  await store.setDefaultModel(MODEL);

  assert.deepEqual(calls, [{ id: OLLAMA, modelId: MODEL }]);
  assert.equal(store.detail?.defaultModelId, MODEL);
  assert.equal(store.selectedSummary?.defaultModelId, MODEL);
});

test("creating a provider keeps the probe result attached to the saved form", async () => {
  const { bridge, store } = storeWith();
  bridge.handle("providers.create", (input) => provider({ name: input.name }));
  bridge.handle("providers.probe", () => ({
    providerId: null,
    outcome: { kind: "ok" as const, latencyMs: 120, live: true, models: [] },
    status: "ok" as const,
    statusDetail: null,
    checkedAt: 1_700_000_100_000,
    provider: null,
  }));
  await store.load();
  store.startCreate();
  store.form?.setName("Новое подключение");
  store.form?.setSecretId(SECRET);
  await store.probe();

  const saved = await store.submit();

  assert.equal(saved, true);
  assert.equal(store.probeResult?.outcome.kind, "ok");
  assert.equal(store.detail?.id, OLLAMA);
});

test("a failing host leaves the page usable and maps the copy", async () => {
  const bridge = createFakeBridge<Contract>();
  bridge.fail("providers.adapters", AppErrorCode.DB_ERROR, "raw host text");
  const store = new ProviderStore(createIpcClient(contract, bridge));

  await store.load();

  assert.equal(store.loaded, true);
  assert.equal(store.error, "Сбой локальной базы данных.");
});

test("removing the selected provider clears the detail column", async () => {
  const { bridge, store } = storeWith();
  bridge.handle("providers.remove", ({ id }) => ({ id, removed: true as const }));
  await store.load();

  const removed = await store.removeSelected();

  assert.equal(removed, true);
  assert.equal(store.selectedId, null);
  assert.equal(store.form, null);
});
