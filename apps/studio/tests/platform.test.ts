import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { createLogger } from "../src/host/platform/logger.ts";
import { createId } from "../src/host/platform/ids.ts";
import {
  cacheDir,
  dbPath,
  logsDir,
  resourcesDir,
  logFilePath,
} from "../src/host/platform/paths.ts";

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
  const directory = mkdtempSync(join(tmpdir(), "studio-logger-"));
  try {
    const options = {
      directory,
      level: "info" as const,
      development: false,
      maxBytes: 100,
      backups: 2,
      now: () => 123,
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
    rmSync(directory, { recursive: true, force: true });
  }
});

test("entity ids are unique, ordered UUID v7 values", () => {
  const ids = Array.from({ length: 1000 }, createId);
  assert.equal(new Set(ids).size, ids.length);
  assert.deepEqual(ids, [...ids].sort());
  for (const id of ids)
    assert.match(id, /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
});
