import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "vitest";
import {
  ADAPTER_FAMILIES,
  AppError,
  AppErrorCode,
  isAppError,
  type AdapterFamily,
  type HostEvent,
} from "@zvs/shared";
import { createFakeDriver } from "../../../test/helpers/FakeDriver.ts";
import { createFakeTransport, sseFrames } from "../../../test/helpers/FakeTransport.ts";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { createSecretService } from "../../../test/helpers/secretService.ts";
import {
  ADAPTER_REGISTRY,
  adapterCapabilities,
  adapterEntry,
} from "../src/host/drivers/ai/adapters/index.ts";
import { OpenAiCompatibleAdapter } from "../src/host/drivers/ai/adapters/openaiCompatible.ts";
import { sseData } from "../src/host/drivers/ai/adapters/sse.ts";
import { unsupportedParameters } from "../src/host/drivers/ai/AdapterCapabilities.ts";
import {
  httpFailure,
  networkFailure,
  rawErrorText,
  toAppError,
} from "../src/host/drivers/ai/errors.ts";
import { ProviderRegistry } from "../src/host/drivers/ai/ProviderRegistry.ts";
import { streamToEvents } from "../src/host/drivers/ai/streamToEvents.ts";
import { ApiTransport } from "../src/host/drivers/ai/transport/ApiTransport.ts";
import { joinUrl } from "../src/host/drivers/ai/transport/Transport.ts";
import { createEventBus, type StreamHandle } from "../src/host/platform/events.ts";
import type { ProviderDraft, Repositories } from "../src/host/data/repositories/index.ts";
import type { GenerateRequest } from "../src/host/drivers/ai/ports.ts";

let database: TemporaryDatabase;
let repositories: Repositories;

const draft: ProviderDraft = {
  kind: "ollama",
  name: "Local",
  baseUrl: "http://localhost:11434/v1",
  capabilities: ["text"],
  createdAt: 100,
  updatedAt: 100,
};

const request: GenerateRequest = {
  model: "llama3",
  messages: [{ role: "user", content: "Привет" }],
  temperature: 0.4,
  topP: 0.9,
  topK: 40,
  maxOutputTokens: 128,
};

beforeEach(() => {
  database = temporaryDatabase();
  repositories = database.client.repositories;
});

afterEach(() => database.dispose());

function collect(): { events: HostEvent[]; stream: StreamHandle } {
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

function tokens(events: readonly HostEvent[]): string[] {
  return events.flatMap((event) => (event.type === "token" ? [event.delta] : []));
}

function registry(): ProviderRegistry {
  return new ProviderRegistry({
    providers: repositories.providers,
    secrets: createSecretService(database.client),
  });
}

test("migration 0003 adds adapter and auth mode and backfills from kind", () => {
  const columns = database.client.db.$client.prepare("PRAGMA table_info(provider)").all() as {
    name: string;
    dflt_value: string | null;
    notnull: number;
  }[];
  const adapter = columns.find((column) => column.name === "adapter");
  const authMode = columns.find((column) => column.name === "auth_mode");
  assert.equal(adapter?.notnull, 1);
  assert.equal(adapter?.dflt_value, "'openai-compatible'");
  assert.equal(authMode?.dflt_value, "'api'");

  const local = repositories.providers.create(draft);
  const qwen = repositories.providers.create({ ...draft, kind: "openai-compatible", name: "Qwen" });
  assert.equal(repositories.providers.findById(local.id)?.adapter, "openai-compatible");
  assert.equal(repositories.providers.findById(local.id)?.authMode, "api");
  assert.equal(qwen.adapter, "openai-compatible");
  repositories.providers.update(qwen.id, { adapter: "qwen-web" });
  assert.equal(repositories.providers.findById(qwen.id)?.adapter, "qwen-web");
});

test("every registered adapter family is implemented, and an unknown one is refused", () => {
  assert.deepEqual(Object.keys(ADAPTER_REGISTRY).sort(), [
    "deepseek-web",
    "openai-compatible",
    "qwen-web",
  ]);
  for (const family of ADAPTER_FAMILIES) {
    const driver = adapterEntry(family).build({ transport: createFakeTransport() });
    assert.equal(adapterEntry(family).implemented, true);
    assert.notEqual(driver.text, null);
  }
  assert.throws(
    () => adapterEntry("made-up" as AdapterFamily),
    (error: unknown) =>
      isAppError(error) &&
      error.code === AppErrorCode.UNKNOWN &&
      error.message === "adapter made-up is not implemented yet",
  );
  assert.equal(adapterCapabilities("qwen-web").authModes.includes("account"), true);
  assert.equal(adapterCapabilities("openai-compatible").honours.topK, false);
});

test("the registry builds a driver per adapter value, caches it and invalidates on change", async () => {
  const row = repositories.providers.create(draft);
  const providers = registry();
  const first = await providers.driver(row.id);
  assert.equal(first, await providers.driver(row.id));
  assert.equal(providers.size, 1);

  repositories.providers.update(row.id, { baseUrl: "http://localhost:11434/v1/", updatedAt: 200 });
  const second = await providers.driver(row.id);
  assert.notEqual(first, second);

  providers.invalidate(row.id);
  assert.equal(providers.size, 0);
  assert.notEqual(second, await providers.driver(row.id));

  const stray = repositories.providers.create({ ...draft, name: "Stray" });
  repositories.providers.update(stray.id, { adapter: "made-up" as AdapterFamily });
  await assert.rejects(
    () => providers.driver(stray.id),
    (error: unknown) =>
      isAppError(error) && error.message === "adapter made-up is not implemented yet",
  );
});

test("the registry resolves the secret at point of use and drops the driver when it changes", async () => {
  const secrets = createSecretService(database.client);
  const key = secrets.create({ type: "custom", name: "Key", scope: "personal", value: "sk-first" });
  const row = repositories.providers.create({
    ...draft,
    kind: "openrouter",
    name: "Router",
    baseUrl: "https://openrouter.ai/api/v1",
    secretId: key.id,
  });
  const calls: { headers: Record<string, string> }[] = [];
  const providers = new ProviderRegistry({
    providers: repositories.providers,
    secrets,
    fetch: async (_url, init) => {
      calls.push({ headers: init.headers as Record<string, string> });
      return new Response(JSON.stringify({ data: [{ id: "openai/gpt-4o" }] }), { status: 200 });
    },
  });

  const text = await providers.text(row.id);
  const models = await text.listModels(AbortSignal.timeout(5000));
  assert.deepEqual(
    models.map((model) => model.externalId),
    ["openai/gpt-4o"],
  );
  assert.equal(calls[0]?.headers.authorization, "Bearer sk-first");

  secrets.update(key.id, { value: "sk-second" });
  providers.invalidateBySecret(key.id);
  assert.equal(providers.size, 0);
  const refreshed = await providers.text(row.id);
  await refreshed.listModels(AbortSignal.timeout(5000));
  assert.equal(calls.at(-1)?.headers.authorization, "Bearer sk-second");
});

test("a disabled or missing provider is refused before any transport is built", async () => {
  const row = repositories.providers.create({ ...draft, enabled: false });
  const providers = registry();
  await assert.rejects(
    () => providers.driver(row.id),
    (error: unknown) => isAppError(error) && error.code === AppErrorCode.CONFLICT,
  );
  await assert.rejects(
    () => providers.driver("missing"),
    (error: unknown) => isAppError(error) && error.code === AppErrorCode.NOT_FOUND,
  );
});

test("account auth mode is rejected with a clear message rather than a silent fallback", async () => {
  const row = repositories.providers.create(draft);
  database.client.db.$client
    .prepare("UPDATE provider SET auth_mode = 'account' WHERE id = ?")
    .run(row.id);
  await assert.rejects(
    () => registry().driver(row.id),
    (error: unknown) =>
      isAppError(error) &&
      error.code === AppErrorCode.VALIDATION_FAILED &&
      error.message === "adapter openai-compatible cannot run in account mode",
  );
});

test("the openai-compatible adapter maps a request onto chat completions and drops top K", async () => {
  const transport = createFakeTransport();
  const logged: { message: string; fields: Record<string, unknown> | undefined }[] = [];
  const adapter = new OpenAiCompatibleAdapter({
    transport,
    logger: {
      log: (_level, _scope, message, fields) => {
        logged.push({ message, fields });
      },
      close: () => undefined,
    },
  });
  transport.reply("/chat/completions", {
    body: {
      model: "llama3",
      choices: [{ message: { content: "Здравствуйте" }, finish_reason: "stop" }],
      usage: { prompt_tokens: 7, completion_tokens: 3, total_tokens: 10 },
    },
  });

  const result = await adapter.generate(
    { ...request, system: "Будь краток" },
    new AbortController().signal,
  );
  assert.equal(result.text, "Здравствуйте");
  assert.equal(result.finishReason, "stop");
  assert.deepEqual(result.usage, { promptTokens: 7, completionTokens: 3, totalTokens: 10 });

  const body = transport.lastBody<Record<string, unknown>>();
  assert.equal(body.temperature, 0.4);
  assert.equal(body.top_p, 0.9);
  assert.equal(body.max_tokens, 128);
  assert.equal("top_k" in body, false);
  assert.equal("topK" in body, false);
  assert.deepEqual(body.messages, [
    { role: "system", content: "Будь краток" },
    { role: "user", content: "Привет" },
  ]);
  assert.deepEqual(logged[0]?.fields?.dropped, ["topK"]);
});

test("the openai-compatible adapter turns SSE frames into deltas and maps discovery onto model rows", async () => {
  const transport = createFakeTransport();
  const adapter = new OpenAiCompatibleAdapter({ transport });
  transport.reply("/chat/completions", { chunks: sseFrames(["При", "вет", "!"]) });
  transport.reply("/models", {
    body: {
      data: [
        {
          id: "openai/gpt-4o",
          name: "GPT-4o",
          context_length: 128000,
          owned_by: "openai",
          supported_parameters: ["tools", "reasoning"],
          architecture: { input_modalities: ["text", "image"] },
          top_provider: { max_completion_tokens: 16384 },
        },
        { id: "llama3:8b" },
        { id: "llama3:8b" },
        { id: "  " },
      ],
    },
  });

  const deltas: string[] = [];
  for await (const delta of adapter.stream(request, new AbortController().signal)) {
    deltas.push(delta.text);
  }
  assert.deepEqual(deltas, ["При", "вет", "!"]);
  assert.equal(transport.calls.at(-1)?.streamed, true);
  assert.equal(transport.lastBody<{ stream: boolean }>().stream, true);

  const models = await adapter.listModels(new AbortController().signal);
  assert.equal(models.length, 2);
  assert.deepEqual(models[0], {
    isFree: null,
    noTraining: null,
    externalId: "openai/gpt-4o",
    displayName: "GPT-4o",
    family: "openai",
    contextWindow: 128000,
    maxOutput: 16384,
    sizeBytes: null,
    capabilities: ["streaming", "tools", "reasoning", "vision"],
  });
  assert.deepEqual(models[1]?.capabilities, ["streaming"]);
  assert.equal(models[1]?.family, "llama3");
});

test("OpenRouter catalog joins on the variant slug and requires explicit privacy flags", async () => {
  const transport = createFakeTransport("https://openrouter.ai/api/v1");
  const adapter = new OpenAiCompatibleAdapter({ transport });
  const policies = [
    { retainsPrompts: false, training: false, trainingOpenRouter: false },
    { retainsPrompt: false, training: false, trainingOpenRouter: false },
    { retainsPrompts: true, training: false, trainingOpenRouter: false },
    { retainsPrompts: false, training: true, trainingOpenRouter: false },
    { retainsPrompts: false, training: false, trainingOpenRouter: true },
    { retainsPrompts: false, training: false },
  ];
  transport.reply("/models", {
    body: {
      data: [
        ...policies.map((_, index) => ({ id: `author/model-${String(index)}:free` })),
        { id: "author/unknown:free" },
        { id: "author/model-0" },
      ],
    },
  });
  transport.reply("../frontend/v1/catalog/models", {
    body: {
      data: policies.map((policy, index) => ({
        slug: `author/model-${String(index)}`,
        endpoint: {
          model_variant_slug: `author/model-${String(index)}:free`,
          is_free: index !== 1,
          data_policy: policy,
        },
      })),
    },
  });
  const models = await adapter.listModels(new AbortController().signal);
  assert.deepEqual(
    models.map((model) => model.noTraining),
    [true, true, false, false, false, false, null, null],
  );
  assert.deepEqual(
    models.map((model) => model.isFree),
    [true, false, true, true, true, true, null, null],
  );
});

test("OpenRouter metadata failures preserve discovery without assuming free or private", async () => {
  const transport = createFakeTransport("https://openrouter.ai/api/v1");
  const adapter = new OpenAiCompatibleAdapter({ transport });
  transport.reply("/models", { body: { data: [{ id: "author/model:free" }] } });
  transport.reply("../frontend/v1/catalog/models", { status: 503, body: {} });
  const models = await adapter.listModels(new AbortController().signal);
  assert.equal(models.length, 1);
  assert.equal(models[0]?.isFree, null);
  assert.equal(models[0]?.noTraining, null);
});

test("a base URL that speaks the OpenAI dialect is listed in one request", async () => {
  const transport = createFakeTransport("https://api.mistral.ai/v1");
  const adapter = new OpenAiCompatibleAdapter({ transport });
  transport.reply("/models", { body: { data: [{ id: "deepseek/deepseek-v4.1-flash" }] } });

  const models = await adapter.listModels(new AbortController().signal);

  assert.deepEqual(
    models.map((entry) => entry.externalId),
    ["deepseek/deepseek-v4.1-flash"],
  );
  assert.deepEqual(
    transport.calls.map((call) => call.path),
    ["/models"],
    "/tags is never asked for when /models answers",
  );
});

test("Ollama's own /tags answers where /models is absent, and carries the size it omits", async () => {
  // Shapes taken from the live endpoints: ollama.com/v1/models reports id and nothing else,
  // ollama.com/api/tags reports name, size and details.
  const transport = createFakeTransport("https://ollama.com/api");
  const adapter = new OpenAiCompatibleAdapter({ transport });
  transport.reply("/models", { status: 404, body: { error: "not found" } });
  transport.reply("/tags", {
    body: {
      models: [
        {
          name: "gpt-oss:120b",
          model: "gpt-oss:120b",
          size: 65_290_180_781,
          details: { family: "", parameter_size: "" },
        },
        { name: "qwen3.5:397b", model: "qwen3.5:397b", size: 1_000, details: { family: "qwen3" } },
        { name: "  ", model: "  " },
      ],
    },
  });

  const models = await adapter.listModels(new AbortController().signal);

  assert.deepEqual(
    transport.calls.map((call) => call.path),
    ["/models", "/tags"],
  );
  assert.deepEqual(models[0], {
    isFree: null,
    noTraining: null,
    externalId: "gpt-oss:120b",
    displayName: "gpt-oss:120b",
    family: "gpt-oss",
    contextWindow: null,
    maxOutput: null,
    sizeBytes: 65_290_180_781,
    capabilities: ["streaming"],
  });
  assert.equal(models[1]?.family, "qwen3", "the vendor's own family wins over the id prefix");
  assert.equal(models.length, 2, "a nameless row is dropped, exactly as on the OpenAI path");
});

test("a rejected key is reported from the first path, never retried against the second", async () => {
  const transport = createFakeTransport("https://api.mistral.ai/v1");
  const adapter = new OpenAiCompatibleAdapter({ transport });
  transport.reply("/models", { status: 401, body: { detail: "Invalid API Key" } });

  await assert.rejects(
    () => adapter.listModels(new AbortController().signal),
    (error: unknown) => isAppError(error) && error.code === AppErrorCode.PROVIDER_AUTH_FAILED,
  );
  assert.deepEqual(
    transport.calls.map((call) => call.path),
    ["/models"],
  );
});

test("a vendor that has neither endpoint reports the second failure, not an empty list", async () => {
  const transport = createFakeTransport();
  const adapter = new OpenAiCompatibleAdapter({ transport });
  transport.reply("/models", { status: 404, body: {} });
  transport.reply("/tags", { status: 404, body: {} });

  await assert.rejects(
    () => adapter.listModels(new AbortController().signal),
    (error: unknown) => isAppError(error) && error.details?.status === 404,
  );
});

test("the SSE reader tolerates split chunks, CRLF frames, comments and a missing terminator", async () => {
  async function* chunks(): AsyncIterable<Uint8Array> {
    const encoder = new TextEncoder();
    yield encoder.encode(': ping\r\n\r\ndata: {"a":1}\r\n\r\ndata: {"b"');
    yield encoder.encode(':2}\n\nevent: other\ndata: {"c":3}');
  }
  const seen: string[] = [];
  for await (const payload of sseData(chunks())) seen.push(payload);
  assert.deepEqual(seen, ['{"a":1}', '{"b":2}', '{"c":3}']);
});

test("each vendor failure class maps onto one AppErrorCode", () => {
  assert.equal(httpFailure({ status: 401 }).code, AppErrorCode.PROVIDER_AUTH_FAILED);
  assert.equal(httpFailure({ status: 403 }).code, AppErrorCode.PROVIDER_AUTH_FAILED);
  const limited = httpFailure({ status: 429, headers: { "retry-after": "30" } });
  assert.equal(limited.code, AppErrorCode.RATE_LIMITED);
  assert.equal(limited.details?.retryAfter, 30);
  assert.equal(httpFailure({ status: 504 }).code, AppErrorCode.PROVIDER_UNREACHABLE);
  assert.equal(httpFailure({ status: 503 }).code, AppErrorCode.PROVIDER_UNREACHABLE);
  assert.equal(httpFailure({ status: 418 }).code, AppErrorCode.UNKNOWN);

  const teapot = httpFailure({ status: 418, body: JSON.stringify({ error: { message: "нет" } }) });
  assert.equal(teapot.details?.vendorMessage, "нет");
  assert.equal(rawErrorText(teapot), "нет");
  const body = "Vendor error ".repeat(100);
  assert.equal(rawErrorText(httpFailure({ status: 500, body })), body);
  assert.equal(
    rawErrorText(new Error("fetch failed", { cause: new Error("ECONNRESET") })),
    "fetch failed\nCaused by: ECONNRESET",
  );
  assert.equal(rawErrorText("plain warning"), "plain warning");
  const cyclic = new Error("cyclic");
  cyclic.cause = cyclic;
  assert.equal(rawErrorText(cyclic), "cyclic");
  assert.match(teapot.message, /неизвестную/);

  for (const code of ["ECONNREFUSED", "ENOTFOUND", "ETIMEDOUT"]) {
    const error = Object.assign(new Error("fetch failed"), {
      cause: Object.assign(new Error(code), { code }),
    });
    assert.equal(networkFailure(error).code, AppErrorCode.PROVIDER_UNREACHABLE);
  }
  const abort = Object.assign(new Error("aborted"), { name: "AbortError" });
  assert.equal(networkFailure(abort).code, AppErrorCode.RUN_CANCELLED);
  assert.equal(networkFailure(new Error("boom")).code, AppErrorCode.UNKNOWN);

  const existing = new AppError(AppErrorCode.SECRET_MISSING, "нет значения");
  assert.equal(toAppError(existing), existing);
  assert.equal(AppErrorCode.PROVIDER_SESSION_EXPIRED, "PROVIDER_SESSION_EXPIRED");
});

test("a non 2xx response never reaches the adapter as data", async () => {
  const transport = createFakeTransport();
  const adapter = new OpenAiCompatibleAdapter({ transport });
  transport.reply("/models", { status: 401, body: { error: "bad key" } });
  await assert.rejects(
    () => adapter.listModels(new AbortController().signal),
    (error: unknown) =>
      isAppError(error) &&
      error.code === AppErrorCode.PROVIDER_AUTH_FAILED &&
      error.details?.vendorMessage === "bad key",
  );
  transport.reply("/chat/completions", {
    status: 429,
    headers: { "retry-after": "5" },
    chunks: [],
  });
  await assert.rejects(
    async () => {
      for await (const _delta of adapter.stream(request, new AbortController().signal)) void _delta;
    },
    (error: unknown) => isAppError(error) && error.code === AppErrorCode.RATE_LIMITED,
  );
});

test("the api transport joins the base url, sends the bearer header and honours the timeout", async () => {
  assert.equal(joinUrl("https://x.dev/v1/", "/models"), "https://x.dev/v1/models");
  assert.equal(joinUrl("https://x.dev/v1", "models", { a: "1" }), "https://x.dev/v1/models?a=1");

  const seen: { url: string; init: RequestInit }[] = [];
  const transport = new ApiTransport({
    baseUrl: "https://x.dev/v1",
    apiKey: "  sk-key  ",
    timeoutSeconds: 30,
    fetch: async (url, init) => {
      seen.push({ url, init });
      return new Response('{"ok":true}', { status: 200, headers: { "x-trace": "1" } });
    },
  });
  const response = await transport.request(
    { method: "POST", path: "models", body: { a: 1 } },
    new AbortController().signal,
  );
  assert.equal(response.status, 200);
  assert.deepEqual(response.json<{ ok: boolean }>(), { ok: true });
  assert.equal(response.headers["x-trace"], "1");
  assert.equal(seen[0]?.url, "https://x.dev/v1/models");
  const headers = seen[0]?.init.headers as Record<string, string>;
  assert.equal(headers.authorization, "Bearer sk-key");
  assert.equal(headers["content-type"], "application/json");
  assert.equal(seen[0]?.init.body, '{"a":1}');

  const timing = new ApiTransport({
    baseUrl: "https://x.dev/v1",
    timeoutSeconds: 0.01,
    fetch: async (_url, init) =>
      await new Promise<Response>((_resolve, reject) => {
        init.signal?.addEventListener("abort", () =>
          reject(Object.assign(new Error("aborted"), { name: "AbortError" })),
        );
      }),
  });
  await assert.rejects(
    () => timing.request({ method: "GET", path: "models" }, new AbortController().signal),
    (error: unknown) => isAppError(error) && error.code === AppErrorCode.PROVIDER_UNREACHABLE,
  );
});

test("the api transport sends no authorization header when the provider has no secret", async () => {
  let headers: Record<string, string> = {};
  const transport = new ApiTransport({
    baseUrl: "http://localhost:11434/v1",
    fetch: async (_url, init) => {
      headers = init.headers as Record<string, string>;
      return new Response("{}", { status: 200 });
    },
  });
  await transport.request({ method: "GET", path: "models" }, new AbortController().signal);
  assert.equal("authorization" in headers, false);
});

test("aborting mid stream cancels the vendor request and ends the stream as cancelled", async () => {
  const controller = new AbortController();
  const driver = createFakeDriver({ deltas: ["one", "two", "three"], manual: true });
  const { events, stream } = collect();
  const pump = streamToEvents(stream, driver.text.stream(request, controller.signal));

  await driver.whenHolding();
  driver.release();
  await driver.whenHolding();
  controller.abort();
  const result = await pump;

  assert.equal(result.outcome.status, "cancelled");
  assert.equal(result.outcome.code, AppErrorCode.RUN_CANCELLED);
  assert.equal(driver.emitted, 1);
  assert.equal(result.text, "one");
  assert.equal(stream.closed, true);
  assert.deepEqual(tokens(events), ["one"]);
  assert.equal(events.at(-1)?.type, "end");
});

test("streaming re-emits deltas as token events and closes with the outcome", async () => {
  const driver = createFakeDriver({ deltas: ["При", "вет"] });
  const ok = collect();
  const result = await streamToEvents(
    ok.stream,
    driver.text.stream(request, new AbortController().signal),
  );
  assert.equal(result.text, "Привет");
  assert.deepEqual(tokens(ok.events), ["При", "вет"]);
  const closing = ok.events.at(-1);
  assert.equal(closing?.type, "end");
  assert.deepEqual(closing?.type === "end" ? closing.outcome : null, { status: "ok" });

  const failing = createFakeDriver({
    deltas: ["a", "b"],
    failAfter: 1,
    failWith: new AppError(AppErrorCode.RATE_LIMITED, "слишком часто"),
  });
  const bad = collect();
  const outcome = await streamToEvents(
    bad.stream,
    failing.text.stream(request, new AbortController().signal),
  );
  assert.deepEqual(outcome.outcome, {
    status: "failed",
    code: AppErrorCode.RATE_LIMITED,
    message: "слишком часто",
  });
  assert.equal(outcome.text, "a");
});

test("the fake driver satisfies all three ports and reports unsupported parameters", async () => {
  const driver = createFakeDriver({
    models: [
      {
        externalId: "m",
        displayName: "M",
        family: null,
        contextWindow: null,
        maxOutput: null,
        sizeBytes: null,
        capabilities: [],
      },
    ],
    vectors: [[0.5, 0.25]],
    images: [{ mimeType: "image/png", data: new Uint8Array([1]) }],
  });
  const signal = new AbortController().signal;
  assert.equal((await driver.text.generate(request, signal)).text, "Hello world");
  assert.equal((await driver.embedding.embed(["a"], "e", signal))[0]?.length, 2);
  assert.equal(driver.embedding.dimensions(), 2);
  assert.equal((await driver.image.generateImages({ model: "i", prompt: "p" }, signal)).length, 1);
  assert.equal((await driver.text.listModels(signal)).length, 1);
  assert.deepEqual(
    driver.requests.map((entry) => entry.model),
    ["llama3"],
  );

  assert.deepEqual(
    unsupportedParameters(driver.text.capabilities(), {
      temperature: 0.1,
      topK: 5,
      topP: undefined,
    }),
    ["topK"],
  );
});

test("aborting a transport stream aborts the vendor request itself", async () => {
  const controller = new AbortController();
  let vendorSignal: AbortSignal | undefined;
  const transport = new ApiTransport({
    baseUrl: "https://x.dev/v1",
    fetch: async (_url, init) => {
      vendorSignal = init.signal ?? undefined;
      const body = new ReadableStream<Uint8Array>({
        start(source) {
          source.enqueue(
            new TextEncoder().encode('data: {"choices":[{"delta":{"content":"a"}}]}\n\n'),
          );
        },
      });
      return new Response(body, { status: 200 });
    },
  });
  const adapter = new OpenAiCompatibleAdapter({ transport });

  const seen: string[] = [];
  await assert.rejects(
    async () => {
      for await (const delta of adapter.stream(request, controller.signal)) {
        seen.push(delta.text);
        controller.abort();
      }
    },
    (error: unknown) => isAppError(error) && error.code === AppErrorCode.RUN_CANCELLED,
  );
  assert.deepEqual(seen, ["a"]);
  assert.equal(vendorSignal?.aborted, true);
});
