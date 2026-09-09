import assert from "node:assert/strict";
import { test } from "vitest";
import {
  AppErrorCode,
  contract,
  createBrandedId,
  isAppError,
  SECRET_TYPE_KEYS,
  SECRET_TYPE_REGISTRY,
  SecretId,
  type CreateSecretInput,
} from "@zvs/shared";
import { createSecretHandlers } from "../src/host/ipc/secrets.ts";
import { CryptoService } from "../src/host/services/CryptoService.ts";
import { SecretService } from "../src/host/services/SecretService.ts";
import { createFakeClock } from "../../../test/helpers/fakeClock.ts";
import { createFakeCrypto } from "../../../test/helpers/fakeCrypto.ts";
import { temporaryDatabase } from "../../../test/helpers/tempDb.ts";

const VALUE = "osk_live_0a1b2c3d4e5f60718293a4b5c6d7e8f4f2a";
const MISSING = createBrandedId(SecretId, "01992958-f480-7000-8000-0000000000ff");

function harness() {
  const database = temporaryDatabase();
  const service = new SecretService({
    data: database.client,
    crypto: new CryptoService(createFakeCrypto()),
    clock: createFakeClock(),
  });
  return {
    service,
    handlers: createSecretHandlers(service),
    dispose: () => database.dispose(),
  };
}

function draft(overrides: Partial<CreateSecretInput> = {}): CreateSecretInput {
  return {
    type: "ollama-cloud",
    name: "Ollama Cloud — personal",
    scope: "personal",
    value: VALUE,
    fields: { organization: "zvs-lab" },
    tags: ["llm"],
    ...overrides,
  };
}

function codeOf(fn: () => unknown): AppErrorCode {
  try {
    fn();
  } catch (error: unknown) {
    if (isAppError(error)) return error.code;
    throw error;
  }
  throw new Error("expected a failure");
}

test("secrets.types returns the whole registry and matches the contract", async () => {
  const { handlers, dispose } = harness();
  try {
    const types = await handlers["secrets.types"]();
    assert.deepEqual(
      types.map((schema) => schema.key),
      [...SECRET_TYPE_KEYS],
    );
    assert.deepEqual(contract["secrets.types"].output.parse(types), types);
  } finally {
    dispose();
  }
});

test("secrets.create stores the schema fields, applies defaults and returns no value", async () => {
  const { handlers, dispose } = harness();
  try {
    const created = await handlers["secrets.create"](draft());

    assert.equal(Object.hasOwn(created, "value"), false);
    assert.equal(Object.hasOwn(created, "cipher"), false);
    assert.equal(JSON.stringify(created).includes(VALUE), false);
    assert.equal(created.hint, "osk_live_…4f2a");
    assert.deepEqual(created.fields, {
      baseUrl: "https://ollama.com/api",
      organization: "zvs-lab",
      verifyTls: true,
    });
    assert.equal(created.usageCount, 0);
    assert.equal(created.rotationStatus, "ok");
    assert.deepEqual(contract["secrets.create"].output.parse(created), created);
  } finally {
    dispose();
  }
});

test("secrets.create rejects an unknown field, an unknown type and a missing credential", async () => {
  const { handlers, dispose } = harness();
  try {
    assert.equal(
      codeOf(() => handlers["secrets.create"](draft({ fields: { sneaky: "value" } }))),
      AppErrorCode.VALIDATION_FAILED,
    );
    assert.equal(
      codeOf(() => handlers["secrets.create"](draft({ type: "aws" as CreateSecretInput["type"] }))),
      AppErrorCode.VALIDATION_FAILED,
    );
    assert.equal(
      codeOf(() => handlers["secrets.create"](draft({ value: "   " }))),
      AppErrorCode.VALIDATION_FAILED,
    );
    assert.deepEqual(await handlers["secrets.list"]({}), []);
  } finally {
    dispose();
  }
});

test("secrets.create demands a credential for every type in the registry", async () => {
  const { handlers, dispose } = harness();
  try {
    for (const schema of SECRET_TYPE_REGISTRY) {
      assert.equal(
        codeOf(() =>
          handlers["secrets.create"]({
            type: schema.key as CreateSecretInput["type"],
            name: `Без значения — ${schema.key}`,
            scope: "personal",
            fields: {},
            tags: [],
          }),
        ),
        AppErrorCode.VALIDATION_FAILED,
        schema.key,
      );
    }
  } finally {
    dispose();
  }
});

test("secrets.list and secrets.get carry the hint and never a value", async () => {
  const { handlers, dispose } = harness();
  try {
    const created = await handlers["secrets.create"](draft());
    const listed = await handlers["secrets.list"]({ scope: "personal" });

    assert.equal(listed.length, 1);
    const [first] = listed;
    assert.equal(first?.hint, "osk_live_…4f2a");
    assert.equal(Object.hasOwn(first ?? {}, "fields"), false);
    assert.equal(JSON.stringify(listed).includes(VALUE), false);
    assert.deepEqual(contract["secrets.list"].output.parse(listed), listed);

    const fetched = await handlers["secrets.get"]({ id: created.id });
    assert.deepEqual(fetched, created);
    assert.deepEqual(await handlers["secrets.list"]({ type: "mistral" }), []);
    assert.equal(
      codeOf(() => handlers["secrets.get"]({ id: MISSING })),
      AppErrorCode.NOT_FOUND,
    );
  } finally {
    dispose();
  }
});

test("secrets.update without a value leaves the credential alone", async () => {
  const { handlers, dispose } = harness();
  try {
    const created = await handlers["secrets.create"](draft());
    const renamed = await handlers["secrets.update"]({ id: created.id, name: "Ollama — work" });

    assert.equal(renamed.name, "Ollama — work");
    assert.equal(renamed.hint, created.hint);
    assert.deepEqual(renamed.fields, created.fields);

    const replaced = await handlers["secrets.update"]({
      id: created.id,
      value: "osk_live_ffffffffffffffffffffffffffffffff9999",
    });
    assert.equal(replaced.hint, "osk_live_…9999");
    assert.equal(
      codeOf(() => handlers["secrets.update"]({ id: MISSING, name: "x" })),
      AppErrorCode.NOT_FOUND,
    );
  } finally {
    dispose();
  }
});

test("secrets.update validates replacement fields against the stored type schema", async () => {
  const { handlers, dispose } = harness();
  try {
    const created = await handlers["secrets.create"](draft());
    assert.equal(
      codeOf(() => handlers["secrets.update"]({ id: created.id, fields: { sneaky: 1 } })),
      AppErrorCode.VALIDATION_FAILED,
    );
    const updated = await handlers["secrets.update"]({
      id: created.id,
      fields: { verifyTls: false },
    });
    assert.deepEqual(updated.fields, {
      baseUrl: "https://ollama.com/api",
      verifyTls: false,
    });
  } finally {
    dispose();
  }
});

test("secrets.remove deletes an unused secret and names the consumers of a used one", async () => {
  const { handlers, service, dispose } = harness();
  try {
    const created = await handlers["secrets.create"](draft());
    service.addUsage(created.id, "provider", "provider-1");
    service.addUsage(created.id, "vector_store", "store-1");

    assert.equal((await handlers["secrets.get"]({ id: created.id })).usageCount, 2);

    try {
      handlers["secrets.remove"]({ id: created.id });
      throw new Error("expected a conflict");
    } catch (error: unknown) {
      assert.equal(isAppError(error), true);
      if (!isAppError(error)) throw error;
      assert.equal(error.code, AppErrorCode.CONFLICT);
      assert.deepEqual(error.details?.consumers, [
        { kind: "provider", id: "provider-1" },
        { kind: "vector_store", id: "store-1" },
      ]);
    }

    service.removeUsage(created.id, "provider", "provider-1");
    service.removeUsage(created.id, "vector_store", "store-1");
    const removed = await handlers["secrets.remove"]({ id: created.id });
    assert.deepEqual(removed, { id: created.id, removed: true });
    assert.deepEqual(await handlers["secrets.list"]({}), []);
    assert.equal(
      codeOf(() => handlers["secrets.remove"]({ id: MISSING })),
      AppErrorCode.NOT_FOUND,
    );
  } finally {
    dispose();
  }
});
