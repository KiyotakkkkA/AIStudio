import { delimiter, join } from "node:path";
import { expect, test } from "vitest";
import { resolveWindowsCommand, windowsLaunch } from "../src/host/drivers/mcp/windowsCommand.ts";
import { isolatedToolchainEnv } from "../src/host/drivers/mcp/toolchainEnv.ts";

const NODE_DIR = join("C:", "Program Files", "nodejs");
const TOOLS_DIR = join("C:", "tools");
const WORK = join("C:", "work");
const COMSPEC = "C:\\Windows\\system32\\cmd.exe";
const files = new Set([
  join(NODE_DIR, "npx.cmd"),
  join(NODE_DIR, "npx"),
  join(NODE_DIR, "node.exe"),
  join(TOOLS_DIR, "uvx.exe"),
]);
const exists = (path: string) => files.has(path);
const env = { Path: [NODE_DIR, TOOLS_DIR].join(delimiter), PATHEXT: ".COM;.EXE;.BAT;.CMD" };

test("a bare command resolves through PATH and PATHEXT, preferring the shim over the bash script", () => {
  expect(resolveWindowsCommand("npx", env, exists, WORK)).toBe(join(NODE_DIR, "npx.cmd"));
  expect(resolveWindowsCommand("uvx", env, exists, WORK)).toBe(join(TOOLS_DIR, "uvx.exe"));
  expect(resolveWindowsCommand("node.exe", env, exists, WORK)).toBe(join(NODE_DIR, "node.exe"));
  expect(resolveWindowsCommand("missing", env, exists, WORK)).toBeNull();
});

test("an executable is spawned directly with its arguments untouched", () => {
  expect(windowsLaunch("uvx", ["mcp-server-time"], env, exists, WORK)).toEqual({
    file: join(TOOLS_DIR, "uvx.exe"),
    args: ["mcp-server-time"],
    verbatim: false,
  });
});

test("a .cmd shim runs through cmd.exe with every argument quoted and metacharacters escaped", () => {
  const launch = windowsLaunch(
    "npx",
    ["-y", "@modelcontextprotocol/server-filesystem", "C:\\My Files", 'a"b', "x&calc"],
    { ...env, ComSpec: COMSPEC },
    exists,
    WORK,
  );
  expect(launch.file).toBe(COMSPEC);
  expect(launch.verbatim).toBe(true);
  expect(launch.args.slice(0, 3)).toEqual(["/d", "/s", "/c"]);
  const line = launch.args[3]!;
  expect(line.startsWith('"') && line.endsWith('"')).toBe(true);
  expect(line).toContain("npx.cmd");
  expect(line).toContain('^"@modelcontextprotocol/server-filesystem^"');
  expect(line).toContain('^"C:\\My^ Files^"');
  expect(line).toContain('^"a\\^"b^"');
  expect(line).toContain('^"x^&calc^"');
  expect(line).not.toMatch(/[^^]&/);
});

test("the toolchain env keeps npx and uvx downloads inside the app folder", () => {
  const root = join("C:", "data", "mcp-toolchain");
  const isolated = isolatedToolchainEnv(root);
  for (const value of [
    isolated.npm_config_cache,
    isolated.UV_CACHE_DIR,
    isolated.UV_TOOL_DIR,
    isolated.UV_PYTHON_INSTALL_DIR,
  ])
    expect(value?.startsWith(root)).toBe(true);
});
