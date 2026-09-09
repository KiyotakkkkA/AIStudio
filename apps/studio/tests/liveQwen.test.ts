import assert from "node:assert/strict";
import { test } from "vitest";
import { QwenWebAdapter } from "../src/host/drivers/ai/adapters/qwenWeb.ts";
import { mapQwenIdentity } from "../src/host/drivers/ai/identity/qwen.ts";
import { AccountTransport } from "../src/host/drivers/ai/transport/AccountTransport.ts";
import type { SessionGateway } from "../src/host/drivers/ai/transport/SessionGateway.ts";

// Opt in explicitly: this creates up to two vendor chats. Never log credentials or bodies.
const token = process.env.LIVE_QWEN_TOKEN ?? "";

test.runIf(token.length > 0)(
  "live qwen: identity, models and Thinking/Fast generation",
  {
    timeout: 180_000,
  },
  async () => {
    const session: SessionGateway = {
      partition: "live-qwen-test",
      userAgent: () =>
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36",
      fetch: async (url, request) => {
        // This direct test has no Electron cookie jar or browser verification state.
        const response = await fetch(url, { ...request, redirect: "error" });
        console.log(
          `Qwen ${request.method} ${new URL(url).pathname}: ${response.status} ${response.headers.get("content-type")}`,
        );
        return response;
      },
    };
    const credentials = {
      current: async () => ({ token, tokenType: "Bearer" }),
      refresh: async () => null,
    };
    const identityTransport = new AccountTransport({
      baseUrl: "https://chat.qwen.ai/api/v1",
      session,
      credentials,
    });
    const response = await identityTransport.request(
      { method: "GET", path: "/auths/" },
      AbortSignal.timeout(30_000),
    );
    const identity = mapQwenIdentity(JSON.parse(response.text) as unknown);
    assert.ok(identity.identity.externalId.length > 0);
    const adapter = new QwenWebAdapter({
      transport: new AccountTransport({
        baseUrl: "https://chat.qwen.ai/api/v2",
        session,
        credentials,
      }),
    });
    const models = await adapter.listModels(AbortSignal.timeout(30_000));
    assert.ok(models.length > 0);
    assert.ok(models.every((model) => !model.capabilities.includes("vision")));
    console.log(`Qwen discovered ${models.length} models`);
    for (const thinking of [true, false]) {
      const model = models.find((entry) => entry.capabilities.includes("reasoning") === thinking);
      assert.ok(model, `No ${thinking ? "Thinking" : "Fast"} model available`);
      const result = await adapter.generate(
        {
          model: model.externalId,
          messages: [{ role: "user", content: "Reply with just the word Hello." }],
        },
        AbortSignal.timeout(60_000),
      );
      assert.equal(result.finishReason, "stop");
      assert.ok(result.text.trim().length > 0);
      console.log(
        `Qwen ${thinking ? "Thinking" : "Fast"}: completed, answer characters=${result.text.length}`,
      );
    }
  },
);
