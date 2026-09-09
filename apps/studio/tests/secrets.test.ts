import assert from "node:assert/strict";
import { eq } from "drizzle-orm";
import { test } from "vitest";
import { AppErrorCode, isAppError } from "@zvs/shared";
import { secret } from "../src/host/data/schema/index.ts";
import type { SecretDraft } from "../src/host/data/repositories/index.ts";
import { createFakeClock } from "../../../test/helpers/fakeClock.ts";
import { temporaryDatabase } from "../../../test/helpers/tempDb.ts";

const NOW = createFakeClock().now();

function draft(overrides: Partial<SecretDraft> = {}): SecretDraft {
  return {
    type: "ollama",
    name: "Ollama Cloud — personal",
    scope: "personal",
    cipher: Buffer.from("ciphertext"),
    hint: "…4f2a",
    fields: JSON.stringify({ baseUrl: "https://ollama.com/api" }),
    tags: JSON.stringify(["llm", "cloud"]),
    createdAt: NOW,
    updatedAt: NOW,
    ...overrides,
  };
}

test("a secret round-trips through create, read, update and delete", () => {
  const database = temporaryDatabase();
  try {
    const secrets = database.client.repositories.secrets;
    const created = secrets.create(draft());

    assert.equal(typeof created.id, "string");
    assert.equal(created.name, "Ollama Cloud — personal");
    assert.equal(created.cipherVersion, 1);
    assert.equal(created.hint, "…4f2a");
    assert.deepEqual(JSON.parse(created.tags), ["llm", "cloud"]);
    assert.deepEqual(secrets.findById(created.id), created);

    const updated = secrets.update(created.id, { name: "Ollama Cloud — lab", updatedAt: NOW + 10 });
    assert.equal(updated?.name, "Ollama Cloud — lab");
    assert.equal(updated?.updatedAt, NOW + 10);
    assert.equal(secrets.listSummaries().length, 1);

    secrets.remove(created.id);
    assert.equal(secrets.findById(created.id), undefined);
    assert.deepEqual(secrets.listSummaries(), []);
  } finally {
    database.dispose();
  }
});

test("the database refuses two secrets with the same name and type", () => {
  const database = temporaryDatabase();
  try {
    const secrets = database.client.repositories.secrets;
    secrets.create(draft());

    assert.throws(
      () => secrets.create(draft()),
      (error: unknown) => /UNIQUE constraint failed/.test(String(error)),
    );

    const other = secrets.create(draft({ type: "custom" }));
    assert.equal(other.type, "custom");
    assert.equal(secrets.listSummaries().length, 2);
  } finally {
    database.dispose();
  }
});

test("only findCipherById returns ciphertext", () => {
  const database = temporaryDatabase();
  try {
    const secrets = database.client.repositories.secrets;
    const created = secrets.create(draft());

    const readPaths: unknown[] = [
      ...secrets.listSummaries(),
      ...secrets.listSummaries({ query: "ollama" }),
      ...secrets.listRotatingBefore(Number.MAX_SAFE_INTEGER),
      secrets.findById(created.id),
      secrets.update(created.id, { note: "no cipher here" }),
      secrets.create(draft({ name: "second" })),
    ];
    for (const row of readPaths) {
      assert.notEqual(row, undefined);
      assert.equal(Object.hasOwn(row as object, "cipher"), false);
    }

    const cipher = secrets.findCipherById(created.id);
    assert.equal(cipher?.cipherVersion, 1);
    assert.equal(cipher?.cipher?.toString(), "ciphertext");
    assert.equal(secrets.findCipherById("missing"), undefined);
  } finally {
    database.dispose();
  }
});

test("deleting a secret something still uses is refused and names the consumers", () => {
  const database = temporaryDatabase();
  try {
    const secrets = database.client.repositories.secrets;
    const created = secrets.create(draft());
    secrets.addUsage({
      secretId: created.id,
      consumerKind: "provider",
      consumerId: "provider-1",
      createdAt: NOW,
    });

    assert.throws(
      () => secrets.remove(created.id),
      (error: unknown) => {
        assert.equal(isAppError(error), true);
        assert.equal(isAppError(error) && error.code, AppErrorCode.CONFLICT);
        assert.deepEqual(isAppError(error) ? error.details : undefined, {
          secretId: created.id,
          consumers: [{ kind: "provider", id: "provider-1" }],
        });
        return true;
      },
    );
    assert.notEqual(secrets.findById(created.id), undefined);

    assert.throws(
      () => database.client.db.delete(secret).where(eq(secret.id, created.id)).run(),
      (error: unknown) => /FOREIGN KEY constraint failed/.test(String(error)),
    );

    secrets.removeUsage(created.id, "provider", "provider-1");
    secrets.remove(created.id);
    assert.equal(secrets.findById(created.id), undefined);
  } finally {
    database.dispose();
  }
});

test("usage is counted per secret across every consumer kind", () => {
  const database = temporaryDatabase();
  try {
    const secrets = database.client.repositories.secrets;
    const first = secrets.create(draft());
    const second = secrets.create(draft({ type: "openrouter", name: "OpenRouter — main" }));

    const consumers = [
      ["provider", "provider-1"],
      ["provider", "provider-2"],
      ["vector_store", "store-1"],
      ["mcp_server", "mcp-1"],
      ["integration", "integration-1"],
    ] as const;
    for (const [consumerKind, consumerId] of consumers) {
      secrets.addUsage({ secretId: first.id, consumerKind, consumerId, createdAt: NOW });
    }
    secrets.addUsage({
      secretId: first.id,
      consumerKind: "provider",
      consumerId: "provider-1",
      createdAt: NOW + 5,
    });

    assert.equal(secrets.countUsage(first.id), 5);
    assert.equal(secrets.countUsage(second.id), 0);
    assert.deepEqual(
      secrets.listUsage(first.id).map((entry) => `${entry.consumerKind}:${entry.consumerId}`),
      [
        "integration:integration-1",
        "mcp_server:mcp-1",
        "provider:provider-1",
        "provider:provider-2",
        "vector_store:store-1",
      ],
    );
    assert.equal(
      secrets.listUsage(first.id).find((entry) => entry.consumerId === "provider-1")?.createdAt,
      NOW + 5,
    );

    secrets.removeUsage(first.id, "provider", "provider-2");
    assert.equal(secrets.countUsage(first.id), 4);

    secrets.remove(second.id);
    assert.equal(secrets.listSummaries().length, 1);
  } finally {
    database.dispose();
  }
});

test("summaries are filtered by scope, type and name, newest first", () => {
  const database = temporaryDatabase();
  try {
    const secrets = database.client.repositories.secrets;
    secrets.create(draft({ updatedAt: NOW + 1 }));
    secrets.create(
      draft({ type: "openrouter", name: "OpenRouter — main", scope: "shared", updatedAt: NOW + 3 }),
    );
    secrets.create(draft({ type: "mistral", name: "Mistral — embeddings", updatedAt: NOW + 2 }));

    assert.deepEqual(
      secrets.listSummaries().map((row) => row.type),
      ["openrouter", "mistral", "ollama"],
    );
    assert.deepEqual(
      secrets.listSummaries({ scope: "shared" }).map((row) => row.name),
      ["OpenRouter — main"],
    );
    assert.deepEqual(
      secrets.listSummaries({ type: "mistral" }).map((row) => row.name),
      ["Mistral — embeddings"],
    );
    assert.deepEqual(
      secrets.listSummaries({ query: "ollama" }).map((row) => row.type),
      ["ollama"],
    );
    assert.deepEqual(secrets.listSummaries({ query: "%" }), []);
    assert.equal(secrets.listSummaries({ query: "   " }).length, 3);
  } finally {
    database.dispose();
  }
});

test("rotating secrets are listed by due date and undated ones are left out", () => {
  const database = temporaryDatabase();
  try {
    const secrets = database.client.repositories.secrets;
    secrets.create(draft({ name: "never rotates" }));
    secrets.create(
      draft({ type: "mistral", name: "due later", rotationDays: 90, rotatesAt: NOW + 2000 }),
    );
    secrets.create(
      draft({ type: "custom", name: "due sooner", rotationDays: 30, rotatesAt: NOW + 1000 }),
    );

    assert.deepEqual(
      secrets.listRotatingBefore(NOW + 3000).map((row) => row.name),
      ["due sooner", "due later"],
    );
    assert.deepEqual(
      secrets.listRotatingBefore(NOW + 1000).map((row) => row.name),
      ["due sooner"],
    );
    assert.deepEqual(secrets.listRotatingBefore(NOW), []);
  } finally {
    database.dispose();
  }
});

test("a rolled-back unit of work leaves no secret and no usage behind", () => {
  const database = temporaryDatabase();
  try {
    const secrets = database.client.repositories.secrets;
    assert.throws(() =>
      database.client.transaction((repositories) => {
        const created = repositories.secrets.create(draft());
        repositories.secrets.addUsage({
          secretId: created.id,
          consumerKind: "provider",
          consumerId: "provider-1",
          createdAt: NOW,
        });
        throw new Error("the unit of work failed halfway");
      }),
    );
    assert.deepEqual(secrets.listSummaries(), []);
  } finally {
    database.dispose();
  }
});
