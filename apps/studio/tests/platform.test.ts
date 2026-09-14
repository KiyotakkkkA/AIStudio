import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "vitest";
import { createLogger } from "../src/host/platform/logger.ts";
import { createId } from "../src/host/platform/ids.ts";
import { DeviceProbe, deviceTier } from "../src/host/platform/device.ts";
import {
  cacheDir,
  dbPath,
  logsDir,
  resourcesDir,
  logFilePath,
} from "../src/host/platform/paths.ts";
import { createFakeClock } from "../../../test/helpers/fakeClock.ts";
import { temporaryDirectory } from "../../../test/helpers/paths.ts";

test("platform paths separate persistent data from bundled resources", () => {
  const env = {
    userData: join(tmpdir(), "user"),
    resources: join(tmpdir(), "bundle"),
    appRoot: join(tmpdir(), "app"),
    packaged: false,
  };
  assert.equal(dbPath(env), join(env.userData, "studio.sqlite"));
  assert.equal(logsDir(env), join(env.userData, "logs"));
  assert.equal(cacheDir(env), join(env.userData, "cache"));
  assert.equal(resourcesDir(env), join(env.appRoot, "resources"));
  assert.equal(resourcesDir({ ...env, packaged: true }), env.resources);
});

test("logger filters, preserves scope, rotates, resumes and closes", () => {
  const temp = temporaryDirectory("studio-logger-");
  const directory = temp.path;
  try {
    const options = {
      directory,
      level: "info" as const,
      development: false,
      maxBytes: 100,
      backups: 2,
      now: createFakeClock(123),
    };
    const logger = createLogger(options);
    logger.log("debug", "test", "filtered");
    for (let index = 0; index < 5; index++)
      logger.log("info", "test", `line ${index}`, { scope: "spoof" });
    assert.equal(readdirSync(directory).length, 3);
    assert.deepEqual(JSON.parse(readFileSync(logFilePath(directory), "utf8")), {
      timestamp: 123,
      level: "info",
      scope: "test",
      message: "line 4",
    });
    logger.close();
    logger.log("error", "test", "after close");
    const resumed = createLogger(options);
    resumed.log("warn", "test", "resumed");
    resumed.close();
    assert.match(readFileSync(logFilePath(directory, 1), "utf8"), /line 4/);
    assert.match(readFileSync(logFilePath(directory), "utf8"), /resumed/);
  } finally {
    temp.dispose();
  }
});

test("warning and error logs place redacted raw text immediately after message", () => {
  const temp = temporaryDirectory("studio-logger-raw-");
  try {
    const logger = createLogger({ directory: temp.path, level: "debug", development: false });
    for (const level of ["warn", "error"] as const) {
      logger.log(level, "providers", "Probe did not succeed", {
        providerId: "provider-1",
        raw: `Invalid key sk-${"a".repeat(24)}\nVendor detail`,
      });
    }
    logger.log("warn", "ai", "Unknown response format");
    logger.log("info", "providers", "Created a provider");
    logger.close();
    const rows = readFileSync(logFilePath(temp.path), "utf8")
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line) as Record<string, unknown>);
    for (const row of rows.slice(0, 2)) {
      assert.equal(row.raw, "Invalid key [redacted]\nVendor detail");
      const keys = Object.keys(row);
      assert.equal(keys.indexOf("raw"), keys.indexOf("message") + 1);
    }
    assert.equal(rows[2]!.raw, "Unknown response format");
    assert.equal(Object.hasOwn(rows[3]!, "raw"), false);
  } finally {
    temp.dispose();
  }
});

test("entity ids are unique, ordered UUID v7 values", () => {
  const ids = Array.from({ length: 1000 }, createId);
  assert.equal(new Set(ids).size, ids.length);
  assert.deepEqual(ids, [...ids].sort());
  for (const id of ids)
    assert.match(id, /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
});

const READINGS = {
  platform: "win32",
  arch: "x64",
  cpuModel: "Test CPU",
  cpuCores: 8,
  totalMemoryBytes: 16 * 1024 ** 3,
  freeMemoryBytes: 4 * 1024 ** 3,
};

test("the device tier follows memory and cores, and a discrete GPU lifts it one step", () => {
  const gpu = { vendor: "NVIDIA", model: "RTX", vramBytes: null, discrete: true };
  const integrated = { ...gpu, vendor: "Intel", discrete: false };
  assert.equal(deviceTier(READINGS, null), "medium");
  assert.equal(deviceTier(READINGS, integrated), "medium");
  assert.equal(deviceTier(READINGS, gpu), "high");
  const weak = { ...READINGS, cpuCores: 4, totalMemoryBytes: 8 * 1024 ** 3 };
  assert.equal(deviceTier(weak, null), "low");
  assert.equal(deviceTier(weak, gpu), "medium");
  const strong = { ...READINGS, cpuCores: 16, totalMemoryBytes: 64 * 1024 ** 3 };
  assert.equal(deviceTier(strong, null), "high");
});

test("the device profile is measured once and a failing probe costs only its own field", async () => {
  const clock = createFakeClock();
  let reads = 0;
  const probe = new DeviceProbe({
    readings: () => {
      reads += 1;
      return READINGS;
    },
    gpu: () => Promise.reject(new Error("no GPU service")),
    freeDisk: () => Promise.resolve(123.9),
    clock: () => clock.advance(1),
  });
  const first = await probe.profile();
  assert.equal(first.gpu, null);
  assert.equal(first.freeDiskBytes, 123);
  assert.equal(first.tier, "medium");
  assert.equal(first.cpuModel, "Test CPU");

  assert.equal((await probe.profile()).measuredAt, first.measuredAt);
  assert.equal(reads, 1);
  assert.notEqual((await probe.profile(true)).measuredAt, first.measuredAt);
  assert.equal(reads, 2);
});
