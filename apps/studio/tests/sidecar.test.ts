import assert from "node:assert/strict";
import { join } from "node:path";
import { expect, test, vi } from "vitest";
import {
  AppError,
  AppErrorCode,
  RunId,
  StreamId,
  contract,
  type Json,
  type StartRunInput,
} from "@zvs/shared";
import {
  SidecarDriver,
  type SidecarDriverOptions,
  type SidecarJobsPort,
} from "../src/host/drivers/sidecar/SidecarDriver.ts";
import { NodeRegistry } from "../src/host/kernel/NodeRegistry.ts";
import { registerJobNodes } from "../src/host/kernel/jobNodes.ts";
import type { StepContext } from "../src/host/kernel/types.ts";
import { sidecarPath } from "../src/host/platform/paths.ts";
import { createId } from "../src/host/platform/ids.ts";
import { createEventBus } from "../src/host/platform/events.ts";
import { createSystemHandlers } from "../src/host/ipc/system.ts";
import { SystemService } from "../src/host/services/SystemService.ts";
import { REPO_ROOT, STUDIO_ROOT, temporaryDirectory } from "../../../test/helpers/paths.ts";

const FAKE = join(REPO_ROOT, "test", "helpers", "fakeSidecar.mjs");

function driver(options: Partial<SidecarDriverOptions> & { deaf?: boolean } = {}) {
  const { deaf, ...rest } = options;
  const log = vi.fn();
  const instance = new SidecarDriver({
    binaryPath: process.execPath,
    args: deaf ? [FAKE, "deaf"] : [FAKE],
    logger: { log, close() {} },
    backoffMs: 10,
    cancelGraceMs: 150,
    pingIntervalMs: 60_000,
    pingTimeoutMs: 500,
    ...rest,
  });
  return {
    instance,
    log,
    logged: (message: string) => log.mock.calls.some(([, , m]) => m === message),
  };
}

const until = async (condition: () => boolean, label: string): Promise<void> => {
  for (let attempt = 0; attempt < 200; attempt++) {
    if (condition()) return;
    await new Promise((done) => setTimeout(done, 25));
  }
  throw new Error(`Timed out waiting for ${label}`);
};

test("the sidecar binary lives beside the addon, under the target triple", () => {
  const env = { userData: "user", resources: "bundle", appRoot: STUDIO_ROOT, packaged: false };
  assert.equal(
    sidecarPath(env, "win32"),
    join(STUDIO_ROOT, "resources/sidecar/x86_64-pc-windows-msvc/zvs-jobd.exe"),
  );
  assert.equal(
    sidecarPath({ ...env, packaged: true }, "linux"),
    join("bundle/sidecar/x86_64-unknown-linux-gnu/zvs-jobd"),
  );
});

test("a missing binary names the expected path and the build command", async () => {
  const temp = temporaryDirectory("studio-sidecar-");
  try {
    const binaryPath = join(temp.path, "zvs-jobd.exe");
    const sidecar = new SidecarDriver({ binaryPath });
    const error = await sidecar.run("job.sleep", null).catch((cause: unknown) => cause);
    assert.ok(error instanceof AppError);
    assert.equal(error.code, AppErrorCode.SIDECAR_UNAVAILABLE);
    assert.ok(error.message.includes(binaryPath));
    assert.ok(error.message.includes("pnpm build:rust"));
  } finally {
    temp.dispose();
  }
});

test("concurrent jobs are matched to their requests and report progress", async () => {
  const { instance } = driver();
  try {
    const seen: number[] = [];
    const [slow, quick, third] = await Promise.all([
      instance.run("ok", { delayMs: 80, tag: "slow" }),
      instance.run("ok", { delayMs: 0, tag: "quick" }),
      instance.run("ok", { delayMs: 40, tag: "third" }, { onProgress: (p) => seen.push(p.done) }),
    ]);
    for (const [answer, tag] of [
      [slow, "slow"],
      [quick, "quick"],
      [third, "third"],
    ] as const) {
      expect(answer).toMatchObject({ job: "ok", params: { tag } });
    }
    assert.deepEqual(seen, [0, 2]);
  } finally {
    await instance.dispose();
  }
});

test("cancelling a job stops it, and a job that ignores cancellation loses the process", async () => {
  const { instance, logged } = driver();
  try {
    const polite = new AbortController();
    const stopped = instance.run("ok", { delayMs: 10_000 }, { signal: polite.signal });
    await new Promise((done) => setTimeout(done, 50));
    polite.abort();
    await expect(stopped).rejects.toMatchObject({ code: AppErrorCode.RUN_CANCELLED });

    const stubborn = new AbortController();
    const hung = instance.run("hang", null, { signal: stubborn.signal });
    await new Promise((done) => setTimeout(done, 50));
    stubborn.abort();
    await expect(hung).rejects.toMatchObject({ code: AppErrorCode.SIDECAR_UNAVAILABLE });
    assert.ok(logged("A job ignored cancellation; killing the sidecar"));

    expect(await instance.run("ok", { delayMs: 0 })).toMatchObject({ job: "ok" });
  } finally {
    await instance.dispose();
  }
});

test("a crashed sidecar fails its jobs cleanly, is logged, and restarts on the next one", async () => {
  const { instance, log, logged } = driver();
  try {
    const survivor = instance.run("ok", { delayMs: 10_000 });
    await expect(instance.run("boom", null)).rejects.toMatchObject({
      code: AppErrorCode.SIDECAR_UNAVAILABLE,
    });
    await expect(survivor).rejects.toMatchObject({ code: AppErrorCode.SIDECAR_UNAVAILABLE });
    assert.ok(logged("The sidecar exited; it restarts on the next job"));
    const restart = log.mock.calls.find(
      ([, , m]) => m === "The sidecar exited; it restarts on the next job",
    );
    expect(restart?.[3]).toMatchObject({ restarts: 1, backoffMs: 10 });

    expect(await instance.run("ok", { delayMs: 0 })).toMatchObject({ job: "ok" });
  } finally {
    await instance.dispose();
  }
});

test("a sidecar that stops answering ping is killed and replaced", async () => {
  const { instance, logged } = driver({ deaf: true, pingIntervalMs: 50, pingTimeoutMs: 100 });
  try {
    const abandoned = instance.run("hang", null);
    await expect(abandoned).rejects.toMatchObject({ code: AppErrorCode.SIDECAR_UNAVAILABLE });
    await until(
      () => logged("The sidecar stopped answering ping; killing it"),
      "the health check to give up",
    );
    assert.ok(logged("The sidecar exited; it restarts on the next job"));
  } finally {
    await instance.dispose();
  }
});

test("ping round-trips against a healthy sidecar and a disposed driver refuses work", async () => {
  const { instance } = driver();
  await instance.ping();
  await instance.dispose();
  await expect(instance.run("ok", null)).rejects.toMatchObject({ code: AppErrorCode.CONFLICT });
});

test("the demo job starts one kernel run over the sidecar's job.sleep", () => {
  const started: StartRunInput[] = [];
  const handlers = createSystemHandlers({
    system: new SystemService({
      chunk: () => Promise.reject(new Error("unused")),
      hash: () => Promise.reject(new Error("unused")),
    }),
    events: createEventBus(),
    runs: {
      start(input) {
        started.push(input);
        return { id: RunId.parse(createId()), streamId: StreamId.parse(createId()) };
      },
    },
  });
  const handle = handlers["system.demoJob"]({ steps: 5, intervalMs: 20 });
  assert.equal(contract["system.demoJob"].output.safeParse(handle).success, true);
  assert.equal(started.length, 1);
  assert.equal(started[0]!.kind, "job");
  assert.deepEqual(started[0]!.graph.nodes[0]!.input, {
    job: "job.sleep",
    params: { steps: 5, intervalMs: 20 },
  });
  assert.equal(started[0]!.graph.nodes[0]!.type, "job.run");
  assert.equal(contract["system.demoJob"].input.safeParse({ steps: 0 }).success, false);
});

test("a job.run step forwards sidecar progress as kernel progress events", async () => {
  const registry = registerJobNodes(new NodeRegistry());
  const node = registry.get("job.run");
  assert.deepEqual(node.permission, { tool: "job.run" });
  assert.equal(node.sideEffectFree, false);

  const emitted: unknown[] = [];
  const jobs: SidecarJobsPort = {
    async run(job, params, options) {
      options?.onProgress?.({ done: 1, total: 4 });
      options?.onProgress?.({ done: 4, total: 4, message: "готово" });
      return { job, params } as Json;
    },
  };
  const context: StepContext = {
    runId: RunId.parse(createId()),
    signal: new AbortController().signal,
    emit: (event) => emitted.push(event),
    requestApproval: () => Promise.reject(new Error("not asked")),
    services: { jobs },
  };
  const output = await node.execute(context, { job: "job.sleep", params: { steps: 4 } });
  expect(output).toMatchObject({ job: "job.sleep", params: { steps: 4 } });
  assert.deepEqual(emitted, [
    { type: "progress", done: 1, total: 4 },
    { type: "progress", done: 4, total: 4 },
    { type: "log", line: { job: "job.sleep", message: "готово" } },
  ]);

  const orphan: StepContext = { ...context, services: {} };
  await expect(node.execute(orphan, { job: "job.sleep", params: null })).rejects.toMatchObject({
    code: AppErrorCode.CONFLICT,
  });
});
