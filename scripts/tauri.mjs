import { execFileSync, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, URL } from "node:url";
import process from "node:process";

const root = fileURLToPath(new URL("../", import.meta.url));
const localProtoc = join(
  root,
  "target/tools/protoc/bin",
  process.platform === "win32" ? "protoc.exe" : "protoc",
);
if (!process.env.PROTOC && existsSync(localProtoc)) process.env.PROTOC = localProtoc;

const [command, ...rest] = process.argv.slice(2);
if (command !== "dev" && command !== "build") throw new Error("Usage: tauri.mjs <dev|build> [...]");
const release = command === "build" && !rest.includes("--debug");

execFileSync(
  "cargo",
  ["build", "--locked", "--package", "zvs-jobd", ...(release ? ["--release"] : [])],
  { cwd: root, stdio: "inherit" },
);

const tauri = spawnSync("pnpm", ["exec", "tauri", command, ...rest], {
  cwd: join(root, "apps/studio"),
  stdio: "inherit",
  shell: process.platform === "win32",
});
process.exit(tauri.status ?? 1);
