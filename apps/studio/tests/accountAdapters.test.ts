import { readFileSync } from "node:fs";
import { join } from "node:path";
import assert from "node:assert/strict";
import { test } from "vitest";
import { AppErrorCode, isAppError } from "@zvs/shared";
import { createFakeSession } from "../../../test/helpers/FakeSession.ts";
import { createFakeTransport } from "../../../test/helpers/FakeTransport.ts";
import { REPO_ROOT } from "../../../test/helpers/paths.ts";
import {
  ACCOUNT_GENERATION_PENDING,
  type AccountAdapterOptions,
} from "../src/host/drivers/ai/adapters/accountFamily.ts";
import {
  DEEPSEEK_CURATED_MODELS,
  DEEPSEEK_MODELS_OBSERVATION,
  DeepSeekWebAdapter,
  DEEPSEEK_WEB_CAPABILITIES,
} from "../src/host/drivers/ai/adapters/deepseekWeb.ts";
import {
  QWEN_MODELS_OBSERVATION,
  QWEN_MODELS_PATH,
  QwenWebAdapter,
  QWEN_WEB_CAPABILITIES,
} from "../src/host/drivers/ai/adapters/qwenWeb.ts";
import { adapterEntry } from "../src/host/drivers/ai/adapters/index.ts";
import { AccountTransport } from "../src/host/drivers/ai/transport/AccountTransport.ts";
import type { Logger, LogLevel } from "../src/host/platform/logger.ts";
import type { DiscoveredModel } from "../src/host/drivers/ai/ports.ts";

interface LogEntry {
  readonly level: LogLevel;
  readonly scope: string;
  readonly message: string;
  readonly fields: Record<string, unknown> | undefined;
}

const QWEN_BASE_URL = "https://chat.qwen.ai/api/v2";
const QWEN_MODELS_URL = `${QWEN_BASE_URL}${QWEN_MODELS_PATH}`;

function fixture(name: string): unknown {
  const path = join(REPO_ROOT, "test", "fixtures", "providers", name);
  return JSON.parse(readFileSync(path, "utf8")) as unknown;
}

function recordingLogger(): { entries: LogEntry[]; logger: Logger } {
  const entries: LogEntry[] = [];
  return {
    entries,
    logger: {
      log(level, scope, message, fields) {
        entries.push({ level, scope, message, fields });
      },
      close() {},
    },
  };
}

function signal(): AbortSignal {
  return AbortSignal.timeout(5000);
}

test("the qwen family lists models from the recorded payload", async () => {
  const transport = createFakeTransport(QWEN_BASE_URL);
  transport.reply(QWEN_MODELS_PATH, { body: fixture("qwenModels.json") });
  const adapter = new QwenWebAdapter({ transport });

  const models = await adapter.listModels(signal());

  assert.equal(transport.calls.at(-1)?.method, "GET");
  assert.equal(transport.calls.at(-1)?.url, QWEN_MODELS_URL);
  assert.deepEqual(models, [
    {
      externalId: "qwen3.7-plus",
      displayName: "Qwen3.7-Plus",
      family: "qwen",
      contextWindow: 131072,
      maxOutput: 32768,
      sizeBytes: null,
      capabilities: ["streaming", "vision", "reasoning"],
    },
    {
      externalId: "qwen3.5-omni-plus",
      displayName: "Qwen3.5-Omni-Plus",
      family: "qwen",
      contextWindow: 262144,
      maxOutput: 65536,
      sizeBytes: null,
      capabilities: ["streaming", "vision", "reasoning"],
    },
    {
      externalId: "qwen3.5-plus",
      displayName: "Qwen3.5-Plus",
      family: "qwen",
      contextWindow: null,
      maxOutput: null,
      sizeBytes: null,
      capabilities: ["streaming"],
    },
  ] satisfies DiscoveredModel[]);
  assert.equal(QWEN_MODELS_OBSERVATION.observedAt, "2026-09-10");
  assert.equal(QWEN_WEB_CAPABILITIES.liveModelList, true);
});

test("a hostile or malformed qwen payload fails as UNKNOWN without leaking the body", async () => {
  const secret = "eyJhbGciOi.SUPERSECRET.signature";
  for (const body of [
    { success: true, data: { data: [{ id: 42, token: secret }] } },
    { success: true, data: "not-a-list" },
    { success: false, data: { data: [] } },
  ]) {
    const transport = createFakeTransport(QWEN_BASE_URL);
    transport.reply(QWEN_MODELS_PATH, { body });
    const { entries, logger } = recordingLogger();
    const adapter = new QwenWebAdapter({ transport, logger });

    await assert.rejects(
      () => adapter.listModels(signal()),
      (error: unknown) =>
        isAppError(error) &&
        error.code === AppErrorCode.UNKNOWN &&
        error.details?.family === "qwen-web" &&
        !JSON.stringify(error.details).includes(secret),
    );
    assert.equal(entries.length, 1);
    assert.equal(entries[0]?.scope, "ai");
    assert.deepEqual(entries[0]?.fields, { family: "qwen-web", endpoint: QWEN_MODELS_PATH });
    assert.equal(JSON.stringify(entries[0]).includes(secret), false);
  }

  const broken = createFakeTransport(QWEN_BASE_URL);
  broken.reply(QWEN_MODELS_PATH, { text: "<!doctype html>" });
  await assert.rejects(
    () => new QwenWebAdapter({ transport: broken }).listModels(signal()),
    (error: unknown) => isAppError(error) && error.code === AppErrorCode.UNKNOWN,
  );
});

test("a rejected account session surfaces as PROVIDER_SESSION_EXPIRED", async () => {
  const session = createFakeSession();
  session.queue(QWEN_MODELS_URL, { status: 401, body: { detail: "expired" } });
  const adapter = new QwenWebAdapter({
    transport: new AccountTransport({ baseUrl: QWEN_BASE_URL, session }),
  });

  await assert.rejects(
    () => adapter.listModels(signal()),
    (error: unknown) => isAppError(error) && error.code === AppErrorCode.PROVIDER_SESSION_EXPIRED,
  );
});

test("the deepseek family serves a curated list because the vendor exposes no endpoint", async () => {
  const transport = createFakeTransport("https://chat.deepseek.com/api/v0");
  const adapter = new DeepSeekWebAdapter({ transport });

  const models = await adapter.listModels();

  assert.equal(transport.calls.length, 0);
  assert.deepEqual(models, [...DEEPSEEK_CURATED_MODELS]);
  assert.deepEqual(
    models.map((model) => model.externalId),
    ["deepseek-chat", "deepseek-reasoner"],
  );
  assert.deepEqual(models[1]?.capabilities, ["streaming", "reasoning"]);
  assert.equal(
    models.every((model) => model.contextWindow === null && model.maxOutput === null),
    true,
  );
  assert.equal(DEEPSEEK_WEB_CAPABILITIES.liveModelList, false);
  assert.equal(DEEPSEEK_MODELS_OBSERVATION.observedAt, "2026-09-10");
});

test("both account families declare their honoured parameters honestly", () => {
  for (const capabilities of [QWEN_WEB_CAPABILITIES, DEEPSEEK_WEB_CAPABILITIES]) {
    assert.deepEqual(capabilities.authModes, ["account"]);
    assert.equal(capabilities.streaming, true);
    assert.equal(capabilities.embedding, false);
    assert.equal(capabilities.image, false);
    assert.deepEqual(capabilities.honours, {
      temperature: false,
      topK: false,
      topP: false,
      maxOutputTokens: false,
    });
  }
  for (const family of ["qwen-web", "deepseek-web"] as const) {
    const driver = adapterEntry(family).build({ transport: createFakeTransport() });
    assert.equal(driver.embedding, null);
    assert.equal(driver.image, null);
  }
});

test("generation over an account session fails with the documented message", async () => {
  const options: AccountAdapterOptions = { transport: createFakeTransport() };
  for (const adapter of [new QwenWebAdapter(options), new DeepSeekWebAdapter(options)]) {
    const family = adapter.capabilities().family;
    await assert.rejects(
      () => adapter.generate(),
      (error: unknown) =>
        isAppError(error) &&
        error.code === AppErrorCode.UNKNOWN &&
        error.message === ACCOUNT_GENERATION_PENDING &&
        error.details?.family === family,
    );
    await assert.rejects(
      async () => {
        for await (const delta of adapter.stream()) void delta;
      },
      (error: unknown) =>
        isAppError(error) &&
        error.code === AppErrorCode.UNKNOWN &&
        error.message === ACCOUNT_GENERATION_PENDING,
    );
  }
});
