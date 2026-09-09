import assert from "node:assert/strict";
import { test } from "vitest";
import { AppErrorCode, isAppError } from "@zvs/shared";
import { OpenAiCompatibleAdapter } from "../src/host/drivers/ai/adapters/openaiCompatible.ts";
import { ApiTransport } from "../src/host/drivers/ai/transport/ApiTransport.ts";
import { streamToEvents } from "../src/host/drivers/ai/streamToEvents.ts";
import { createEventBus } from "../src/host/platform/events.ts";
import type { HostEvent } from "@zvs/shared";

const baseUrl = process.env.LIVE_BASE_URL ?? "";
const apiKey = process.env.LIVE_API_KEY ?? "";
const wanted =
  process.env.LIVE_MODEL === undefined || process.env.LIVE_MODEL.length === 0
    ? undefined
    : process.env.LIVE_MODEL;
const live = baseUrl.length > 0 && apiKey.length > 0;

function adapter(key: string): OpenAiCompatibleAdapter {
  return new OpenAiCompatibleAdapter({
    transport: new ApiTransport({ baseUrl, apiKey: key, timeoutSeconds: 60 }),
  });
}

test.runIf(live)(
  "live: listModels, streaming, abort and a wrong key",
  { timeout: 120_000 },
  async () => {
    const driver = adapter(apiKey);

    const models = await driver.listModels(AbortSignal.timeout(60_000));
    console.log(`listModels -> ${models.length}`);
    console.log(
      models
        .slice(0, 8)
        .map((model) => `  ${model.externalId} ctx=${model.contextWindow} out=${model.maxOutput}`)
        .join("\n"),
    );

    const model = wanted ?? models[0]?.externalId;
    assert.ok(model !== undefined, "no model to stream with");
    console.log(`streaming with ${model}`);

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
    const full = await streamToEvents(
      bus.openStream(),
      driver.stream(
        {
          model,
          messages: [{ role: "user", content: "Say one short sentence about the sea." }],
          temperature: 0.3,
          topK: 40,
          maxOutputTokens: 512,
        },
        AbortSignal.timeout(60_000),
      ),
    );
    console.log(
      `stream outcome ${JSON.stringify(full.outcome)} tokens=${events.filter((e) => e.type === "token").length}`,
    );
    console.log(`text: ${full.text}`);
    assert.equal(full.outcome.status, "ok");
    assert.ok(full.text.length > 0);

    const controller = new AbortController();
    const stopper = setTimeout(() => controller.abort(), 1200);
    const aborted = await streamToEvents(
      bus.openStream(),
      driver.stream(
        { model, messages: [{ role: "user", content: "Count slowly from 1 to 200." }] },
        controller.signal,
      ),
    );
    clearTimeout(stopper);
    console.log(`abort outcome ${JSON.stringify(aborted.outcome)} partial=${aborted.text.length}`);

    const badKey = adapter("sk-definitely-not-a-real-key");
    const failure = await badKey
      .generate({ model, messages: [{ role: "user", content: "hi" }] }, AbortSignal.timeout(60_000))
      .then(() => null)
      .catch((error: unknown) => error);
    console.log(
      `bad key -> ${isAppError(failure) ? `${failure.code}: ${failure.message}` : String(failure)}`,
    );
    assert.ok(isAppError(failure));
    assert.equal(failure.code, AppErrorCode.PROVIDER_AUTH_FAILED);
  },
);
