import assert from "node:assert/strict";
import { createInterface } from "node:readline";
import process from "node:process";

const write = (value) => process.stdout.write(`${JSON.stringify(value)}\n`);
const lines = createInterface({ input: process.stdin });
let model = "";

for await (const line of lines) {
  const message = JSON.parse(line);
  const { id, method, params } = message;
  if (method === "initialized") continue;
  if (method === "initialize") {
    assert.equal(process.env.ZVS_OPENAI_ACCESS_TOKEN, "fake-oauth-token");
    assert.equal(params.clientInfo.name, "ZVSAIStudio");
    assert.equal(params.capabilities.experimentalApi, true);
    write({ id, result: {} });
  } else if (method === "thread/start") {
    assert.equal(params.ephemeral, true);
    assert.deepEqual(params.environments, []);
    assert.equal(params.approvalPolicy, "untrusted");
    assert.equal(params.sandbox, "read-only");
    model = params.model;
    if (model === "tool" || model === "unoffered") {
      assert.equal(params.dynamicTools[0].type, "namespace");
      assert.equal(params.dynamicTools[0].name, "zvs");
      assert.equal(params.dynamicTools[0].tools[0].type, "function");
    }
    write({ id, result: { thread: { id: "fake-thread" } } });
  } else if (method === "turn/start") {
    if (model.startsWith("effort-")) {
      const effort = model.slice("effort-".length);
      if (effort === "default") assert.equal(Object.hasOwn(params, "effort"), false);
      else assert.equal(params.effort, effort);
    }
    write({ method: "item/agentMessage/delta", params: { delta: "before acknowledgement" } });
    write({ id, result: { turn: { id: "fake-turn" } } });
    if (model === "tool" || model === "unoffered") {
      write({
        id: "server-request",
        method: "item/tool/call",
        params: {
          tool: model === "tool" ? "lookup" : "unoffered",
          namespace: "zvs",
          callId: "call",
          arguments: { query: "value" },
        },
      });
    } else {
      write({
        method: "thread/tokenUsage/updated",
        params: { tokenUsage: { last: { inputTokens: 3, outputTokens: 2, totalTokens: 5 } } },
      });
      write({
        method: "turn/completed",
        params: { turn: { status: model === "failure" ? "failed" : "completed" } },
      });
    }
  } else if (method === "account/rateLimits/read") {
    write({
      id,
      result: {
        rateLimits: { limitId: "codex", primary: { usedPercent: 42, resetsAt: 2000000000 } },
      },
    });
  }
}
