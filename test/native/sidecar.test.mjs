import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath, URL } from "node:url";
import process from "node:process";

const root = fileURLToPath(new URL("../../", import.meta.url));
const triple = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  .match(/^host: (.+)$/m)[1]
  .trim();
const binary = join(
  root,
  "apps/studio/resources/sidecar",
  triple,
  process.platform === "win32" ? "zvs-jobd.exe" : "zvs-jobd",
);

function start() {
  const child = spawn(binary, [], { stdio: ["pipe", "pipe", "pipe"], windowsHide: true });
  const queue = [];
  const waiting = [];
  createInterface({ input: child.stdout }).on("line", (line) => {
    const message = JSON.parse(line);
    const next = waiting.shift();
    if (next) next(message);
    else queue.push(message);
  });
  return {
    child,
    send: (request) => child.stdin.write(`${JSON.stringify(request)}\n`),
    next: () =>
      queue.length
        ? Promise.resolve(queue.shift())
        : new Promise((resolve) => waiting.push(resolve)),
    async settle(id) {
      for (;;) {
        const message = await this.next();
        if (message.id === id && message.type !== "job.progress") return message;
      }
    },
  };
}

test("the released sidecar answers ping, runs the demo job and cancels it", async () => {
  const sidecar = start();
  try {
    sidecar.send({ type: "ping", id: "health" });
    assert.deepEqual(await sidecar.next(), { type: "ping", id: "health" });

    sidecar.send({
      type: "job.start",
      id: "demo",
      job: "job.sleep",
      params: { steps: 3, intervalMs: 10 },
    });
    assert.deepEqual(await sidecar.settle("demo"), {
      type: "job.done",
      id: "demo",
      result: { steps: 3 },
    });

    sidecar.send({
      type: "job.start",
      id: "long",
      job: "job.sleep",
      params: { steps: 100_000, intervalMs: 10 },
    });
    const first = await sidecar.next();
    assert.equal(first.type, "job.progress");
    const at = Date.now();
    sidecar.send({ type: "job.cancel", id: "long" });
    const cancelled = await sidecar.settle("long");
    assert.equal(cancelled.type, "job.error");
    assert.equal(cancelled.code, "RUN_CANCELLED");
    assert.ok(Date.now() - at < 5_000);

    sidecar.send({ type: "job.start", id: "nope", job: "job.nope" });
    assert.equal((await sidecar.settle("nope")).code, "VALIDATION_FAILED");
  } finally {
    sidecar.child.stdin.end();
    await new Promise((done) => sidecar.child.once("exit", done));
  }
});
