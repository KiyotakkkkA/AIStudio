import { readFileSync } from "node:fs";
import { join } from "node:path";
import assert from "node:assert/strict";
import { test } from "vitest";
import { AppErrorCode, isAppError, type AppError, type HostEvent } from "@zvs/shared";
import { createFakeSession } from "../../../test/helpers/FakeSession.ts";
import { createFakeTransport, type FakeTransport } from "../../../test/helpers/FakeTransport.ts";
import { REPO_ROOT } from "../../../test/helpers/paths.ts";
import {
  ACCOUNT_GENERATION_PENDING,
  flattenPrompt,
  type AccountAdapterOptions,
} from "../src/host/drivers/ai/adapters/accountFamily.ts";
import {
  DEEPSEEK_CURATED_MODELS,
  DEEPSEEK_MODELS_OBSERVATION,
  DeepSeekWebAdapter,
  DEEPSEEK_WEB_CAPABILITIES,
} from "../src/host/drivers/ai/adapters/deepseekWeb.ts";
import {
  QWEN_CHATS_NEW_PATH,
  QWEN_COMPLETIONS_PATH,
  QWEN_GENERATION_OBSERVATION,
  QWEN_MODELS_OBSERVATION,
  QWEN_MODELS_PATH,
  QwenWebAdapter,
  mapQwenModels,
  QWEN_WEB_CAPABILITIES,
} from "../src/host/drivers/ai/adapters/qwenWeb.ts";
import { adapterEntry } from "../src/host/drivers/ai/adapters/index.ts";
import { AccountTransport } from "../src/host/drivers/ai/transport/AccountTransport.ts";
import { streamToEvents } from "../src/host/drivers/ai/streamToEvents.ts";
import { createEventBus, type StreamHandle } from "../src/host/platform/events.ts";
import type { Logger, LogLevel } from "../src/host/platform/logger.ts";
import type { GenerateRequest, TextDelta, DiscoveredModel } from "../src/host/drivers/ai/ports.ts";

interface LogEntry {
  readonly level: LogLevel;
  readonly scope: string;
  readonly message: string;
  readonly fields: Record<string, unknown> | undefined;
}

const QWEN_BASE_URL = "https://chat.qwen.ai/api/v2";
const QWEN_MODELS_URL = `${QWEN_BASE_URL}${QWEN_MODELS_PATH}`;

const QWEN_CHAT_ID = "1a77c526-b841-4fef-805c-8096a2b5e4c3";
const FIXED_NOW = 1788994021000;
const FIXED_FID = "b222dc0d-1910-427d-b25e-291a8570e556";

const prompt: GenerateRequest = {
  model: "qwen3.7-plus",
  messages: [{ role: "user", content: "Привет" }],
  temperature: 0.4,
  topK: 40,
};

function fixtureText(name: string): string {
  return readFileSync(join(REPO_ROOT, "test", "fixtures", "providers", name), "utf8");
}

function fixture(name: string): unknown {
  return JSON.parse(fixtureText(name)) as unknown;
}

function sseFixture(name: string): string[] {
  return fixtureText(name)
    .split("\n\n")
    .filter((frame) => frame.trim().length > 0)
    .map((frame) => `${frame}\n\n`);
}

function qwenTransport(stream: string[], delayMs = 0): FakeTransport {
  const transport = createFakeTransport(QWEN_BASE_URL);
  transport.reply(QWEN_MODELS_PATH, { body: fixture("qwenModels.json") });
  transport.reply(QWEN_CHATS_NEW_PATH, { body: fixture("qwenChatCreated.json") });
  transport.reply(QWEN_COMPLETIONS_PATH, { chunks: stream, delayMs });
  return transport;
}

function qwenAdapter(transport: FakeTransport, extra: Partial<AccountAdapterOptions> = {}) {
  return new QwenWebAdapter({
    transport,
    clock: () => FIXED_NOW,
    newId: () => FIXED_FID,
    ...extra,
  });
}

async function drain(deltas: AsyncIterable<TextDelta>): Promise<TextDelta[]> {
  const collected: TextDelta[] = [];
  for await (const delta of deltas) collected.push(delta);
  return collected;
}

function eventStream(): { events: HostEvent[]; stream: StreamHandle } {
  const events: HostEvent[] = [];
  const bus = createEventBus({
    senders: () => [
      {
        isDestroyed: () => false,
        send: (_channel, payload) => {
          events.push(payload as HostEvent);
        },
      },
    ],
  });
  return { events, stream: bus.openStream() };
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
      capabilities: ["streaming", "reasoning"],
    },
    {
      externalId: "qwen3.5-omni-plus",
      displayName: "Qwen3.5-Omni-Plus",
      family: "qwen",
      contextWindow: 262144,
      maxOutput: 65536,
      sizeBytes: null,
      capabilities: ["streaming"],
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
    assert.equal(capabilities.embedding, false);
    assert.equal(capabilities.image, false);
    assert.deepEqual(capabilities.honours, {
      temperature: false,
      topK: false,
      topP: false,
      maxOutputTokens: false,
    });
  }
  assert.equal(QWEN_WEB_CAPABILITIES.streaming, true);
  assert.equal(DEEPSEEK_WEB_CAPABILITIES.streaming, false);
  for (const family of ["qwen-web", "deepseek-web"] as const) {
    const driver = adapterEntry(family).build({ transport: createFakeTransport() });
    assert.equal(driver.embedding, null);
    assert.equal(driver.image, null);
  }
});

test("deepseek generation stays unimplemented until its wire format is recorded", async () => {
  const options: AccountAdapterOptions = { transport: createFakeTransport() };
  const adapter = new DeepSeekWebAdapter(options);
  const rejected = (error: unknown): boolean =>
    isAppError(error) &&
    error.code === AppErrorCode.UNKNOWN &&
    error.message === ACCOUNT_GENERATION_PENDING &&
    error.details?.family === "deepseek-web" &&
    error.details?.model === "deepseek-chat";

  await assert.rejects(() => adapter.generate({ ...prompt, model: "deepseek-chat" }), rejected);
  await assert.rejects(
    () => drain(adapter.stream({ ...prompt, model: "deepseek-chat" })),
    rejected,
  );
  assert.equal(DEEPSEEK_WEB_CAPABILITIES.streaming, false);
});

test("the qwen family opens a chat and streams reasoning and answer as distinct deltas", async () => {
  const transport = qwenTransport(sseFixture("qwenStream.sse"));
  const deltas = await drain(qwenAdapter(transport).stream(prompt, signal()));

  assert.deepEqual(
    deltas.map((delta) => delta.kind),
    ["reasoning", "reasoning", "text", "text", "text"],
  );
  assert.deepEqual(
    deltas.filter((delta) => delta.kind === "text").map((delta) => delta.text),
    ["Привет", ", мир", "!"],
  );
  assert.deepEqual(
    deltas.filter((delta) => delta.kind === "reasoning").map((delta) => delta.text),
    [
      "Пользователь здоровается.\nОтвечу коротким приветствием.",
      "Достаточно одного предложения без лишних деталей.",
    ],
  );

  const [, create, completion] = transport.calls;
  assert.equal(create?.method, "POST");
  assert.equal(create?.url, `${QWEN_BASE_URL}${QWEN_CHATS_NEW_PATH}`);
  assert.deepEqual(create?.body, {
    chatId: "",
    chat_mode: "local",
    chat_type: "t2t",
    models: ["qwen3.7-plus"],
    project_id: "",
    timestamp: FIXED_NOW,
  });
  assert.equal(completion?.streamed, true);
  assert.equal(completion?.url, `${QWEN_BASE_URL}${QWEN_COMPLETIONS_PATH}?chat_id=${QWEN_CHAT_ID}`);

  const body = completion?.body as Record<string, unknown>;
  assert.equal(body.chat_id, QWEN_CHAT_ID);
  assert.equal(body.chatId, QWEN_CHAT_ID);
  assert.equal(body.model, "qwen3.7-plus");
  assert.equal(body.stream, true);
  assert.equal(body.incremental_output, true);
  assert.equal(body.version, "2.1");
  assert.equal(body.parentId, null);
  assert.equal(body.parent_id, null);
  assert.equal(body.timestamp, Math.floor(FIXED_NOW / 1000));

  const [message] = body.messages as Record<string, unknown>[];
  assert.equal(message?.fid, FIXED_FID);
  assert.equal(message?.role, "user");
  assert.equal(message?.content, "Привет");
  assert.equal(message?.parent_id, null);
  assert.deepEqual(message?.models, ["qwen3.7-plus"]);
  assert.equal(QWEN_GENERATION_OBSERVATION.observedAt, "2026-09-10");
});

test("qwen generate collects the answer, the reasoning and the vendor usage", async () => {
  const transport = qwenTransport(sseFixture("qwenStream.sse"));
  const result = await qwenAdapter(transport).generate(prompt, signal());

  assert.equal(result.model, "qwen3.7-plus");
  assert.equal(result.text, "Привет, мир!");
  assert.equal(
    result.reasoning,
    "Пользователь здоровается.\nОтвечу коротким приветствием.Достаточно одного предложения без лишних деталей.",
  );
  assert.equal(result.finishReason, "stop");
  assert.deepEqual(result.usage, { promptTokens: 621, completionTokens: 186, totalTokens: 807 });
});

test("qwen selects thinking from discovery and reuses the model list", async () => {
  const transport = qwenTransport(sseFixture("qwenStream.sse"));
  const adapter = qwenAdapter(transport);
  await adapter.listModels(signal());
  for (const [model, thinking] of [
    ["qwen3.7-plus", true],
    ["qwen3.5-omni-plus", false],
    ["qwen3.5-plus", false],
    ["unlisted-model", false],
  ] as const) {
    await adapter.generate({ ...prompt, model }, signal());
    const body = transport.calls.at(-1)?.body as {
      messages: { feature_config: { thinking_enabled: boolean; thinking_mode: string } }[];
    };
    assert.equal(body.messages[0]?.feature_config.thinking_enabled, thinking);
    assert.equal(body.messages[0]?.feature_config.thinking_mode, thinking ? "Thinking" : "Fast");
  }
  assert.equal(transport.calls.filter((call) => call.url === QWEN_MODELS_URL).length, 1);
});

test("qwen uses legacy thinking only when the explicit capability is absent", () => {
  for (const [capability, ability, expected] of [
    [false, 2, false],
    [0, 2, false],
    [true, 0, true],
    [undefined, 2, true],
    [null, 1, true],
    [undefined, undefined, false],
  ] as const) {
    const models = mapQwenModels({
      data: [
        {
          id: "model",
          info: {
            meta: {
              capabilities: { thinking: capability, vision: true },
              abilities: { thinking: ability },
              modality: ["image", "audio", "video"],
            },
          },
        },
      ],
    });
    assert.deepEqual(
      models?.[0]?.capabilities,
      expected ? ["streaming", "reasoning"] : ["streaming"],
    );
  }
});

for (const status of [401, 403]) {
  test(`qwen standalone SSE status ${status} preserves partial text and expires the session once`, async () => {
    const transport = qwenTransport(sseFixture(`qwenStreamStatus${status}.sse`));
    const seen: AppError[] = [];
    const { stream } = eventStream();
    const result = await streamToEvents(
      stream,
      qwenAdapter(transport, {
        onSessionExpired: (error) => seen.push(error),
      }).stream(prompt, signal()),
    );
    assert.equal(result.text, "Partial");
    assert.equal(result.outcome.code, AppErrorCode.PROVIDER_SESSION_EXPIRED);
    assert.equal(seen.length, 1);
    assert.equal(seen[0]?.details?.status, status);
  });
}

test("qwen recognises standalone authentication codes and numeric strings", async () => {
  for (const code of [401, 403, "401", "403", "session_expired"]) {
    const transport = qwenTransport([`data: ${JSON.stringify({ code })}\n\n`]);
    await assert.rejects(
      () => qwenAdapter(transport).generate(prompt, signal()),
      (error: unknown) => isAppError(error) && error.code === AppErrorCode.PROVIDER_SESSION_EXPIRED,
    );
  }
});

test("qwen allows success status frames but rejects standalone server errors", async () => {
  const successful = qwenTransport(['data: {"status":200}\n\n', ...sseFixture("qwenStream.sse")]);
  assert.equal((await qwenAdapter(successful).generate(prompt, signal())).text, "Привет, мир!");
  const failed = qwenTransport(['data: {"status":500}\n\n']);
  await assert.rejects(
    () => qwenAdapter(failed).generate(prompt, signal()),
    (error: unknown) =>
      isAppError(error) && error.code === AppErrorCode.UNKNOWN && error.details?.status === 500,
  );
});

test("qwen deltas reach the event bus with reasoning kept apart from the answer", async () => {
  const transport = qwenTransport(sseFixture("qwenStream.sse"));
  const { events, stream } = eventStream();

  const result = await streamToEvents(stream, qwenAdapter(transport).stream(prompt, signal()));

  assert.deepEqual(result.outcome, { status: "ok" });
  assert.equal(result.text, "Привет, мир!");
  assert.equal(result.reasoning.length > 0, true);
  const tokens = events.flatMap((event) => (event.type === "token" ? [event] : []));
  assert.deepEqual(
    tokens.filter((event) => event.kind === undefined).map((event) => event.delta),
    ["Привет", ", мир", "!"],
  );
  assert.equal(tokens.filter((event) => event.kind === "reasoning").length, 2);
  assert.equal(events.at(-1)?.type, "end");
});

test("aborting a qwen stream stops emission and ends the run as cancelled", async () => {
  const transport = qwenTransport(sseFixture("qwenStream.sse"), 5);
  const controller = new AbortController();
  const { events, stream } = eventStream();
  const pump = streamToEvents(stream, qwenAdapter(transport).stream(prompt, controller.signal));

  setTimeout(() => controller.abort(), 12);
  const result = await pump;

  assert.equal(result.outcome.status, "cancelled");
  assert.equal(result.outcome.code, AppErrorCode.RUN_CANCELLED);
  assert.equal(transport.aborts > 0, true);
  assert.equal(events.at(-1)?.type, "end");
});

test("a session that expires mid stream keeps the partial text and asks for a re-link", async () => {
  const transport = qwenTransport(sseFixture("qwenStreamExpired.sse"));
  const seen: AppError[] = [];
  const { stream } = eventStream();

  const result = await streamToEvents(
    stream,
    qwenAdapter(transport, { onSessionExpired: (error) => seen.push(error) }).stream(
      prompt,
      signal(),
    ),
  );

  assert.equal(result.text, "Прив");
  assert.equal(result.outcome.status, "failed");
  assert.equal(result.outcome.code, AppErrorCode.PROVIDER_SESSION_EXPIRED);
  assert.equal(seen.length, 1);
  assert.equal(seen[0]?.details?.status, 401);
});

test("a truncated qwen stream is an error rather than a short answer", async () => {
  const transport = qwenTransport(sseFixture("qwenStreamTruncated.sse"));
  const { stream } = eventStream();

  const result = await streamToEvents(stream, qwenAdapter(transport).stream(prompt, signal()));

  assert.equal(result.text, "Привет, мир");
  assert.equal(result.outcome.status, "failed");
  assert.equal(result.outcome.code, AppErrorCode.PROVIDER_UNREACHABLE);
});

test("qwen generation logs dropped parameters and never a request or response body", async () => {
  const transport = qwenTransport(sseFixture("qwenStream.sse"));
  const { entries, logger } = recordingLogger();

  await drain(qwenAdapter(transport, { logger }).stream(prompt, signal()));

  assert.deepEqual(entries, [
    {
      level: "debug",
      scope: "ai",
      message: "Dropped parameters the adapter family does not honour",
      fields: { family: "qwen-web", dropped: ["temperature", "topK"] },
    },
  ]);
  const logged = JSON.stringify(entries);
  for (const secret of ["Привет", QWEN_CHAT_ID, FIXED_FID]) {
    assert.equal(logged.includes(secret), false);
  }
});

test("a malformed chat-create reply fails before any completion request is made", async () => {
  const transport = createFakeTransport(QWEN_BASE_URL);
  transport.reply(QWEN_MODELS_PATH, { body: fixture("qwenModels.json") });
  transport.reply(QWEN_CHATS_NEW_PATH, { body: { success: true, data: { id: "" } } });
  const { entries, logger } = recordingLogger();

  await assert.rejects(
    () => drain(qwenAdapter(transport, { logger }).stream(prompt, signal())),
    (error: unknown) =>
      isAppError(error) &&
      error.code === AppErrorCode.UNKNOWN &&
      error.details?.endpoint === QWEN_CHATS_NEW_PATH,
  );
  assert.equal(transport.calls.length, 2);
  assert.equal(
    entries.some((entry) => JSON.stringify(entry).includes("Привет")),
    false,
  );
});

test("the flattened prompt keeps roles when a turn carries history or a system message", () => {
  assert.equal(flattenPrompt(prompt), "Привет");
  assert.equal(
    flattenPrompt({
      model: "qwen3.7-plus",
      system: "Отвечай кратко",
      messages: [
        { role: "user", content: "Привет" },
        { role: "assistant", content: "Здравствуйте" },
        { role: "user", content: "Как дела?" },
      ],
    }),
    "System: Отвечай кратко\n\nUser: Привет\n\nAssistant: Здравствуйте\n\nUser: Как дела?",
  );
});
