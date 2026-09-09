import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { eq } from "drizzle-orm";
import { test } from "vitest";
import { AppErrorCode, isAppError } from "@zvs/shared";
import { secret } from "../src/host/data/schema/index.ts";
import { createLogger, redactSecrets, REDACTED } from "../src/host/platform/logger.ts";
import { CryptoService } from "../src/host/services/CryptoService.ts";
import { SecretService } from "../src/host/services/SecretService.ts";
import type { SecretCreateInput } from "../src/host/services/SecretService.ts";
import {
  computeHint,
  DUE_SOON_WINDOW_MS,
  rotationDueAt,
  rotationStatus,
} from "../src/host/services/secretPolicy.ts";
import { createFakeClock } from "../../../test/helpers/fakeClock.ts";
import { createFakeCrypto } from "../../../test/helpers/fakeCrypto.ts";
import { temporaryDirectory } from "../../../test/helpers/paths.ts";
import { temporaryDatabase } from "../../../test/helpers/tempDb.ts";

const VALUE = "sk-or-v1-0a1b2c3d4e5f60718293a4b5c6d7e8f9c31";

function harness() {
  const database = temporaryDatabase();
  const clock = createFakeClock();
  const backend = createFakeCrypto();
  const service = new SecretService({
    data: database.client,
    crypto: new CryptoService(backend),
    clock,
  });
  return { database, clock, backend, service, dispose: () => database.dispose() };
}

function input(overrides: Partial<SecretCreateInput> = {}): SecretCreateInput {
  return {
    type: "openrouter",
    name: "OpenRouter — main",
    scope: "personal",
    value: VALUE,
    fields: { baseUrl: "https://openrouter.ai/api/v1" },
    tags: ["llm"],
    ...overrides,
  };
}

test("a created secret round-trips through resolve and never carries its value", async () => {
  const { database, service, dispose } = harness();
  try {
    const created = service.create(input());

    assert.equal(Object.hasOwn(created, "value"), false);
    assert.equal(Object.hasOwn(created, "cipher"), false);
    assert.equal(created.hint, "sk-or-v1-…9c31");
    assert.deepEqual(created.tags, ["llm"]);
    assert.deepEqual(created.fields, { baseUrl: "https://openrouter.ai/api/v1" });
    assert.equal(created.rotation, "ok");

    for (const row of service.list()) {
      assert.equal(Object.hasOwn(row, "value"), false);
      assert.equal(Object.hasOwn(row, "cipher"), false);
      assert.equal(JSON.stringify(row).includes(VALUE), false);
    }

    assert.equal(await service.resolve(created.id), VALUE);

    const stored = database.client.db
      .select({ cipher: secret.cipher })
      .from(secret)
      .where(eq(secret.id, created.id))
      .get();
    assert.equal(stored?.cipher instanceof Buffer, true);
    assert.equal(stored?.cipher?.toString("utf8").includes(VALUE), false);
  } finally {
    dispose();
  }
});

test("updating anything but the value leaves the ciphertext byte-identical", async () => {
  const { database, clock, backend, service, dispose } = harness();
  try {
    const created = service.create(input());
    const before = database.client.repositories.secrets.findCipherById(created.id)?.cipher;
    const encryptCalls = backend.encryptCalls;

    clock.advance(1000);
    const renamed = service.update(created.id, { name: "OpenRouter — lab", note: "shared box" });
    assert.equal(renamed.name, "OpenRouter — lab");
    assert.equal(renamed.updatedAt, created.updatedAt + 1000);
    assert.equal(renamed.hint, created.hint);
    assert.equal(backend.encryptCalls, encryptCalls);
    const after = database.client.repositories.secrets.findCipherById(created.id)?.cipher;
    assert.equal(Buffer.compare(before as Buffer, after as Buffer), 0);
    assert.equal(await service.resolve(created.id), VALUE);

    const replaced = service.update(created.id, { value: "sk-or-v1-ffffffffffffffffffff1234" });
    assert.equal(backend.encryptCalls, encryptCalls + 1);
    assert.equal(replaced.hint, "sk-or-v1-…1234");
    assert.equal(await service.resolve(created.id), "sk-or-v1-ffffffffffffffffffff1234");
    const rotated = database.client.repositories.secrets.findCipherById(created.id)?.cipher;
    assert.notEqual(Buffer.compare(before as Buffer, rotated as Buffer), 0);
  } finally {
    dispose();
  }
});

test("deleting a secret in use is refused and names the consumers", () => {
  const { service, dispose } = harness();
  try {
    const created = service.create(input());
    service.addUsage(created.id, "provider", "provider-1");
    service.addUsage(created.id, "vector_store", "store-1");

    assert.throws(
      () => {
        service.remove(created.id);
      },
      (error: unknown) => {
        assert.equal(isAppError(error) && error.code, AppErrorCode.CONFLICT);
        assert.deepEqual(isAppError(error) ? error.details : undefined, {
          secretId: created.id,
          consumers: [
            { kind: "provider", id: "provider-1" },
            { kind: "vector_store", id: "store-1" },
          ],
        });
        return true;
      },
    );

    service.removeUsage(created.id, "provider", "provider-1");
    service.removeUsage(created.id, "vector_store", "store-1");
    assert.deepEqual(service.usage(created.id), []);
    service.remove(created.id);
    assert.deepEqual(service.list(), []);
  } finally {
    dispose();
  }
});

test("missing secrets and missing ciphertext are reported, not guessed at", async () => {
  const { database, service, dispose } = harness();
  try {
    await assert.rejects(
      () => service.resolve("nope"),
      (error: unknown) => isAppError(error) && error.code === AppErrorCode.NOT_FOUND,
    );
    assert.throws(
      () => service.update("nope", { name: "x" }),
      (error: unknown) => isAppError(error) && error.code === AppErrorCode.NOT_FOUND,
    );
    assert.throws(
      () => {
        service.remove("nope");
      },
      (error: unknown) => isAppError(error) && error.code === AppErrorCode.NOT_FOUND,
    );
    assert.throws(
      () => service.create(input({ value: "   " })),
      (error: unknown) => isAppError(error) && error.code === AppErrorCode.VALIDATION_FAILED,
    );

    const created = service.create(input());
    database.client.db.update(secret).set({ cipher: null }).where(eq(secret.id, created.id)).run();
    await assert.rejects(
      () => service.resolve(created.id),
      (error: unknown) => isAppError(error) && error.code === AppErrorCode.SECRET_MISSING,
    );
  } finally {
    dispose();
  }
});

test("an unavailable keychain fails loudly instead of degrading to plaintext", async () => {
  const { database, backend, service, dispose } = harness();
  try {
    const created = service.create(input());
    backend.available = false;

    assert.throws(
      () => service.create(input({ name: "second" })),
      (error: unknown) => isAppError(error) && error.code === AppErrorCode.SECRET_DECRYPT_FAILED,
    );
    await assert.rejects(
      () => service.resolve(created.id),
      (error: unknown) => isAppError(error) && error.code === AppErrorCode.SECRET_DECRYPT_FAILED,
    );
    assert.equal(database.client.repositories.secrets.listSummaries().length, 1);

    backend.available = true;
    backend.failNext = true;
    await assert.rejects(
      () => service.resolve(created.id),
      (error: unknown) => isAppError(error) && error.code === AppErrorCode.SECRET_DECRYPT_FAILED,
    );

    const unavailable = new CryptoService(createFakeCrypto(false));
    assert.equal(unavailable.isAvailable(), false);
    assert.throws(
      () => unavailable.encrypt("x"),
      (error: unknown) => isAppError(error) && error.code === AppErrorCode.SECRET_DECRYPT_FAILED,
    );
  } finally {
    dispose();
  }
});

test("rotation status turns over at fourteen days and at expiry", () => {
  const { clock, service, dispose } = harness();
  try {
    const now = clock.now();
    assert.equal(rotationStatus(null, now), "ok");
    assert.equal(rotationStatus(now + DUE_SOON_WINDOW_MS + 1, now), "ok");
    assert.equal(rotationStatus(now + DUE_SOON_WINDOW_MS, now), "due-soon");
    assert.equal(rotationStatus(now + 1, now), "due-soon");
    assert.equal(rotationStatus(now, now), "overdue");
    assert.equal(rotationStatus(now - 1, now), "overdue");

    assert.equal(rotationDueAt(null, now), null);
    assert.equal(rotationDueAt(0, now), null);
    assert.equal(rotationDueAt(30, now), now + 30 * 24 * 60 * 60 * 1000);

    const calm = service.create(input({ rotatesAt: now + DUE_SOON_WINDOW_MS + 1 }));
    assert.equal(calm.rotation, "ok");
    const derived = service.create(input({ name: "derived", rotationDays: 90 }));
    assert.equal(derived.rotatesAt, now + 90 * 24 * 60 * 60 * 1000);

    clock.advance(1);
    assert.equal(service.list({ query: "main" })[0]?.rotation, "due-soon");
    clock.set(calm.rotatesAt as number);
    assert.equal(service.list({ query: "main" })[0]?.rotation, "overdue");
  } finally {
    dispose();
  }
});

test("the hint keeps at most four characters of the value", () => {
  assert.equal(computeHint(VALUE), "sk-or-v1-…9c31");
  assert.equal(computeHint("sk-ant-api03-aaaaaaaaaaaaaaaa7d10"), "sk-ant-api03-…7d10");
  assert.equal(computeHint("osk_live_aaaaaaaaaaaa4f2a"), "osk_live_…4f2a");
  assert.equal(computeHint("plain-password-9c31"), "…9c31");
  assert.equal(computeHint("ab"), "…ab");
  assert.equal(computeHint("sk-ab"), "…k-ab");

  for (const value of [VALUE, "osk_live_aaaaaaaaaaaa4f2a", "plain-password-9c31"]) {
    const hint = computeHint(value);
    const tail = hint.slice(hint.indexOf("…") + 1);
    assert.equal(tail.length <= 4, true);
    assert.equal(hint.includes(value), false);
  }
});

test("the log redactor catches realistic key shapes", () => {
  assert.equal(redactSecrets(VALUE), REDACTED);
  assert.equal(redactSecrets(`Authorization: Bearer ${VALUE}`).includes(VALUE), false);
  assert.equal(redactSecrets(`Bearer ${"a1b2c3d4".repeat(4)}`), REDACTED);
  assert.equal(redactSecrets(`key=sk-ant-api03-${"a".repeat(40)} done`), `key=${REDACTED} done`);
  assert.equal(redactSecrets(`ghp_${"a".repeat(36)}`), REDACTED);
  assert.equal(redactSecrets(`xoxb-1234567890-abcdefghij`), REDACTED);
  assert.equal(redactSecrets("nothing to hide here"), "nothing to hide here");
  assert.deepEqual(redactSecrets({ nested: { list: [VALUE, "safe"] }, count: 2 }), {
    nested: { list: [REDACTED, "safe"] },
    count: 2,
  });
  assert.equal(redactSecrets(Buffer.from(VALUE)), REDACTED);
});

test("a full service run writes no secret value into the log file", async () => {
  const directory = temporaryDirectory("studio-secret-log-");
  const database = temporaryDatabase();
  const logger = createLogger({ directory: directory.path, level: "debug", development: false });
  try {
    const service = new SecretService({
      data: database.client,
      crypto: new CryptoService(createFakeCrypto()),
      logger,
      clock: createFakeClock(),
    });
    const created = service.create(input());
    service.update(created.id, { name: "renamed", value: VALUE });
    await service.resolve(created.id);
    logger.log("error", "secrets", `a careless caller logged ${VALUE}`, { token: VALUE });
    service.remove(created.id);
    logger.close();

    const written = readdirSync(directory.path)
      .map((name) => readFileSync(join(directory.path, name), "utf8"))
      .join("");
    assert.equal(written.length > 0, true);
    assert.equal(written.includes(VALUE), false);
    assert.equal(written.includes("9c31"), false);
    assert.equal(written.includes(REDACTED), true);
  } finally {
    logger.close();
    database.dispose();
    directory.dispose();
  }
});
