import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createRequire } from "node:module";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath, URL } from "node:url";
import { Buffer } from "node:buffer";
import { setInterval, clearInterval } from "node:timers";

const root = fileURLToPath(new URL("../../", import.meta.url));
const triple = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  .match(/^host: (.+)$/m)[1]
  .trim();
const directory = join(root, "apps/studio/resources/native", triple);
const require = createRequire(import.meta.url);
const addon = require(join(directory, "zvs-core.node"));

test("release addon chunks owned Unicode text and hashes binary content asynchronously", async () => {
  assert.equal(addon.debugPanic, undefined);
  const pending = addon.chunkText("Привет 🌍 мир", { size: 2, overlap: 1 });
  assert.ok(pending instanceof Promise);
  const chunks = await pending;
  assert.deepEqual(
    chunks.map((chunk) => chunk.text),
    ["Привет 🌍", "🌍 мир"],
  );
  for (const chunk of chunks) {
    assert.equal(
      Buffer.from("Привет 🌍 мир").subarray(chunk.byteStart, chunk.byteEnd).toString(),
      chunk.text,
    );
    assert.equal(chunk.tokenCount, 2);
  }
  assert.deepEqual(await addon.chunkText("", { size: 2, overlap: 0 }), []);
  const hash = addon.hashBytes(Buffer.from("abc"));
  assert.ok(hash instanceof Promise);
  assert.equal(await hash, "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85");
  await assert.rejects(addon.chunkText("invalid", { size: 0, overlap: 0 }), (error) => {
    assert.equal(JSON.parse(error.message).code, "VALIDATION_FAILED");
    return true;
  });
});

test("debug panic rejects and the same process can make another real native call", async () => {
  const debug = require(join(directory, "debug/zvs-core.node"));
  const pending = debug.debugPanic();
  assert.ok(pending instanceof Promise);
  await assert.rejects(pending, (error) => {
    assert.equal(JSON.parse(error.message).code, "NATIVE_ERROR");
    return true;
  });
  assert.equal((await debug.chunkText("still alive", { size: 1, overlap: 0 })).length, 2);
});

test("native work leaves the JavaScript event loop responsive", async () => {
  let ticks = 0;
  const timer = setInterval(() => {
    ticks += 1;
  }, 1);
  try {
    await addon.chunkText("word ".repeat(500_000), { size: 256, overlap: 32 });
    assert.ok(ticks > 0);
  } finally {
    clearInterval(timer);
  }
});
