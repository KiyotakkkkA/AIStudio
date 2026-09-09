import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "vitest";
import { AppErrorCode, isAppError } from "@zvs/shared";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import type { ProviderDraft, Repositories } from "../src/host/data/repositories/index.ts";

let database: TemporaryDatabase;
let repositories: Repositories;
const draft: ProviderDraft = {
  kind: "ollama",
  name: "Local",
  baseUrl: "http://localhost:11434",
  capabilities: ["text"],
  createdAt: 100,
  updatedAt: 100,
};
const discovered = { externalId: "gpt-oss:120b", displayName: "GPT OSS", discoveredAt: 200 };

beforeEach(() => {
  database = temporaryDatabase();
  repositories = database.client.repositories;
});

afterEach(() => database.dispose());

function createSecret(name: string) {
  return repositories.secrets.create({
    name,
    type: "custom",
    scope: "personal",
    createdAt: 100,
    updatedAt: 100,
  });
}

test("capabilities stay synchronized and filters combine with enabled state", () => {
  const { providers } = repositories;
  const text = providers.create(draft);
  const multi = providers.create({
    ...draft,
    name: "Multi",
    capabilities: ["embedding", "image", "image"],
    enabled: false,
  });
  assert.deepEqual(multi.capabilities, ["embedding", "image"]);
  assert.deepEqual(providers.list({ capability: "text" }), [text]);
  assert.deepEqual(providers.list({ capability: "embedding", enabled: false }), [multi]);
  assert.deepEqual(providers.list({ capability: "image", enabled: true }), []);
  assert.equal(providers.list().length, 2);
  providers.update(text.id, {
    capabilities: ["image"],
    settings: { temperature: 0.7, topK: 40, topP: 0.95, maxOutputTokens: 4096, timeoutSeconds: 60 },
  });
  const updated = providers.findById(text.id);
  assert.equal(updated?.capText, false);
  assert.equal(updated?.capEmbedding, false);
  assert.equal(updated?.capImage, true);
  assert.equal(updated?.settings.timeoutSeconds, 60);
  assert.deepEqual(providers.list({ capability: "text" }), []);
  providers.update(text.id, { capabilities: [] });
  assert.equal(providers.findById(text.id)?.capImage, false);
  for (const [capability, column] of [
    ["text", "cap_text"],
    ["embedding", "cap_embedding"],
    ["image", "cap_image"],
  ]) {
    const plan = database.client.db.$client
      .prepare(`EXPLAIN QUERY PLAN SELECT * FROM provider WHERE ${column} = 1`)
      .all();
    assert.match(JSON.stringify(plan), new RegExp(`provider_cap_${capability}_idx`));
  }
});

test("provider names are unique within each kind and failed writes retain secret usage", () => {
  const key = createSecret("Key");
  const row = repositories.providers.create({ ...draft, secretId: key.id });
  assert.throws(() => repositories.providers.create({ ...draft, secretId: key.id }), /UNIQUE/);
  repositories.providers.create({ ...draft, kind: "openai-compatible" });
  assert.equal(repositories.secrets.countUsage(key.id), 1);
  assert.throws(
    () => repositories.providers.update(row.id, { secretId: "missing" }),
    /FOREIGN KEY/,
  );
  assert.equal(repositories.providers.findById(row.id)?.secretId, key.id);
  assert.equal(repositories.secrets.countUsage(key.id), 1);
  assert.throws(
    () => repositories.providers.create({ ...draft, name: "Missing", secretId: "missing" }),
    /FOREIGN KEY/,
  );
  assert.equal(repositories.providers.list().length, 2);
});

test("secret usage follows repointing, clearing, and provider removal", () => {
  const first = createSecret("First");
  const second = createSecret("Second");
  const { providers, secrets } = repositories;
  const row = providers.create({ ...draft, secretId: first.id });
  assert.equal(secrets.listUsage(first.id)[0]?.consumerKind, "provider");
  assert.equal(secrets.listUsage(first.id)[0]?.consumerId, row.id);
  assert.throws(
    () => secrets.remove(first.id),
    (error: unknown) => isAppError(error) && error.code === AppErrorCode.CONFLICT,
  );
  assert.throws(
    () => database.client.db.$client.prepare("DELETE FROM secret WHERE id = ?").run(first.id),
    /FOREIGN KEY/,
  );
  providers.update(row.id, { secretId: second.id });
  assert.equal(secrets.countUsage(first.id), 0);
  assert.equal(secrets.countUsage(second.id), 1);
  providers.update(row.id, { name: "Renamed" });
  assert.equal(secrets.countUsage(second.id), 1);
  providers.update(row.id, { secretId: null });
  assert.equal(secrets.countUsage(second.id), 0);
  providers.update(row.id, { secretId: first.id });
  providers.remove(row.id);
  assert.equal(secrets.countUsage(first.id), 0);
  secrets.remove(first.id);
  assert.equal(secrets.findById(first.id), undefined);
});

test("provider and usage changes participate in an enclosing unit of work", () => {
  const key = createSecret("Key");
  assert.throws(
    () =>
      database.client.transaction(({ providers }) => {
        providers.create({ ...draft, secretId: key.id });
        throw new Error("abort");
      }),
    /abort/,
  );
  assert.deepEqual(repositories.providers.list(), []);
  assert.equal(repositories.secrets.countUsage(key.id), 0);
  const row = repositories.providers.create({ ...draft, secretId: key.id });
  assert.throws(
    () =>
      database.client.transaction(({ providers }) => {
        providers.remove(row.id);
        throw new Error("abort");
      }),
    /abort/,
  );
  assert.equal(repositories.providers.findById(row.id)?.secretId, key.id);
  assert.equal(repositories.secrets.countUsage(key.id), 1);
});

test("discovery replaces stale models and retains model metadata and stable IDs", () => {
  const { providers, models } = repositories;
  const row = providers.create(draft);
  const other = providers.create({ ...draft, name: "Other" });
  const [initial] = models.replaceForProvider(row.id, [
    discovered,
    { ...discovered, externalId: "stale" },
  ]);
  assert.ok(initial);
  models.replaceForProvider(other.id, [discovered]);
  providers.setDefaultModel(row.id, initial.id);
  const [refreshed] = models.replaceForProvider(row.id, [
    {
      ...discovered,
      family: "gpt-oss",
      contextWindow: 128000,
      maxOutput: 4096,
      sizeBytes: 65000000000,
      capabilities: ["tools", "vision", "streaming", "reasoning", "code"],
      available: false,
      unavailableReason: "Not available on this plan",
      discoveredAt: 300,
    },
  ]);
  assert.ok(refreshed);
  assert.equal(refreshed.id, initial.id);
  assert.equal(refreshed.available, false);
  assert.equal(refreshed.unavailableReason, "Not available on this plan");
  assert.equal(refreshed.sizeBytes, 65000000000);
  assert.equal(refreshed.capabilities.length, 5);
  assert.deepEqual(models.findById(refreshed.id), refreshed);
  assert.equal(providers.findById(row.id)?.defaultModelId, initial.id);
  assert.equal(models.listByProvider(other.id).length, 1);
  models.replaceForProvider(row.id, []);
  assert.deepEqual(models.listByProvider(row.id), []);
  assert.equal(providers.findById(row.id)?.defaultModelId, null);
});

test("failed discovery rolls back deletions, inserts, and default selection", () => {
  const { providers, models } = repositories;
  const row = providers.create(draft);
  const previous = models.replaceForProvider(row.id, [discovered]);
  assert.ok(previous[0]);
  providers.setDefaultModel(row.id, previous[0].id);
  const duplicate = { ...discovered, externalId: "duplicate" };
  assert.throws(() => models.replaceForProvider(row.id, [duplicate, duplicate]), /UNIQUE/);
  assert.deepEqual(models.listByProvider(row.id), previous);
  assert.equal(providers.findById(row.id)?.defaultModelId, previous[0].id);
  assert.throws(
    () => models.replaceForProvider("missing", []),
    (error: unknown) => isAppError(error) && error.code === AppErrorCode.NOT_FOUND,
  );
});

test("default models must belong to the provider and deletion cascades models", () => {
  const { providers, models } = repositories;
  const row = providers.create(draft);
  const other = providers.create({ ...draft, name: "Other" });
  const [entry] = models.replaceForProvider(row.id, [discovered]);
  assert.ok(entry);
  assert.throws(
    () => providers.setDefaultModel(other.id, entry.id),
    (error: unknown) => isAppError(error) && error.code === AppErrorCode.CONFLICT,
  );
  assert.throws(() => providers.setDefaultModel(row.id, "missing"));
  providers.setDefaultModel(row.id, entry.id);
  providers.setDefaultModel(row.id, null);
  assert.equal(providers.findById(row.id)?.defaultModelId, null);
  providers.remove(row.id);
  assert.equal(models.findById(entry.id), undefined);
  assert.ok(providers.findById(other.id));
});

test("status records probe time and clears previous details and latency", () => {
  const row = repositories.providers.create(draft);
  assert.equal(row.status, "unknown");
  repositories.providers.setStatus(row.id, "failed", "Connection refused", 412, 200);
  const failed = repositories.providers.findById(row.id);
  assert.equal(failed?.lastProbeAt, 200);
  assert.equal(failed?.lastLatencyMs, 412);
  assert.equal(failed?.statusDetail, "Connection refused");
  const ok = repositories.providers.setStatus(row.id, "ok", null, null, 300);
  assert.equal(ok?.status, "ok");
  assert.equal(ok?.statusDetail, null);
  assert.equal(ok?.lastLatencyMs, null);
  assert.equal(ok?.updatedAt, 300);
  assert.equal(repositories.providers.update("missing", { enabled: false }), undefined);
  assert.equal(repositories.providers.setDefaultModel("missing", null), undefined);
  repositories.providers.remove("missing");
});
