import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { runInNewContext } from "node:vm";
import { expect, test } from "vitest";

const shell = resolve("apps/studio/src-tauri");
const source = readFileSync(resolve(shell, "isolation/index.js"), "utf8");
const filter = runInNewContext(`${source}\nglobalThis.__TAURI_ISOLATION_HOOK__;`) as (
  payload: unknown,
) => unknown;

test("isolation forwards every command granted to an application webview", () => {
  const capabilities = resolve(shell, "capabilities");
  for (const file of readdirSync(capabilities).filter((name) => name.endsWith(".json"))) {
    const capability = JSON.parse(readFileSync(resolve(capabilities, file), "utf8")) as {
      permissions: string[];
    };
    for (const permission of capability.permissions) {
      const command = permission.replace(/^allow-/, "").replaceAll("-", "_");
      const payload = { cmd: command, callback: 1, error: 2, payload: {} };
      expect(filter(payload)).toBe(payload);
    }
  }
});

test("isolation forwards channel fetches for streamed host state", () => {
  const payload = { cmd: "plugin:__TAURI_CHANNEL__|fetch", payload: { id: 1 } };
  expect(filter(payload)).toBe(payload);
});

test.each([null, undefined, "computer_subscribe", {}, { cmd: "unknown_command" }])(
  "isolation rejects unrecognised payload %j",
  (payload) => {
    expect(() => filter(payload)).toThrow();
  },
);
