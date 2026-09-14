import assert from "node:assert/strict";
import { Buffer } from "node:buffer";
import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
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

test("the released sidecar resumes a ranged download and verifies its checksum", async () => {
  const body = Buffer.from("zvs".repeat(4096));
  const digest = createHash("sha256").update(body).digest("hex");
  const ranges = [];
  const origin = createServer((request, response) => {
    const match = /^bytes=(\d+)-/.exec(request.headers.range ?? "");
    const offset = match ? Number(match[1]) : 0;
    if (match) ranges.push(offset);
    const rest = body.subarray(offset);
    response.writeHead(offset > 0 ? 206 : 200, {
      "Accept-Ranges": "bytes",
      "Content-Length": String(rest.length),
      ...(offset > 0
        ? { "Content-Range": `bytes ${offset}-${body.length - 1}/${body.length}` }
        : {}),
    });
    response.end(rest);
  });
  await new Promise((listening) => origin.listen(0, "127.0.0.1", listening));
  const url = `http://127.0.0.1:${origin.address().port}/blob`;
  const directory = mkdtempSync(join(tmpdir(), "zvs-jobd-download-"));
  const target = join(directory, "artefact.bin");
  // Half the artefact is already on disk, as it would be after the app was killed mid-pull.
  writeFileSync(`${target}.part`, body.subarray(0, 6000));

  const sidecar = start();
  try {
    sidecar.send({
      type: "job.start",
      id: "pull",
      job: "job.download",
      params: {
        url,
        targetPath: target,
        checksum: { algorithm: "sha256", value: digest },
      },
    });
    const done = await sidecar.settle("pull");
    assert.equal(done.type, "job.done", JSON.stringify(done));
    assert.equal(done.result.resumedFrom, 6000);
    assert.equal(done.result.bytes, body.length);
    assert.equal(done.result.checksum, digest);
    assert.deepEqual(ranges, [6000]);
    assert.deepEqual(readFileSync(target), body);

    // A digest that does not match must fail and leave nothing behind to resume from.
    sidecar.send({
      type: "job.start",
      id: "corrupt",
      job: "job.download",
      params: {
        url,
        targetPath: join(directory, "corrupt.bin"),
        checksum: { algorithm: "sha256", value: createHash("sha256").update("nope").digest("hex") },
      },
    });
    const failed = await sidecar.settle("corrupt");
    assert.equal(failed.code, "VALIDATION_FAILED");
    assert.equal(existsSync(join(directory, "corrupt.bin")), false);
    assert.equal(existsSync(join(directory, "corrupt.bin.part")), false);
  } finally {
    sidecar.child.stdin.end();
    await new Promise((exited) => sidecar.child.once("exit", exited));
    origin.close();
    rmSync(directory, { recursive: true, force: true });
  }
});
