import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { fileURLToPath, URL } from "node:url";
import process from "node:process";
import { readAppEnv } from "./app-env.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const localProtoc = join(
  root,
  "target/tools/protoc/bin",
  process.platform === "win32" ? "protoc.exe" : "protoc",
);
if (!process.env.PROTOC && existsSync(localProtoc)) process.env.PROTOC = localProtoc;

const [command, ...passed] = process.argv.slice(2);
if (command !== "dev" && command !== "build") throw new Error("Usage: tauri.mjs <dev|build> [...]");
const rest = passed.filter((argument) => argument !== "--");
const release = command === "build" && !rest.includes("--debug");
const features = rest.flatMap((argument, index) => {
  if (argument.startsWith("--features=")) return argument.slice("--features=".length).split(",");
  return argument === "--features" || argument === "-f" ? (rest[index + 1] ?? "").split(",") : [];
});
if (features.includes("e2e")) process.env.ZVS_E2E_BUILD = "1";

const profile = release ? ["--release"] : [];
if (release && process.platform === "win32") {
  process.env.CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = "-C target-feature=+crt-static";
}
execFileSync(
  "cargo",
  ["build", "--locked", "--package", "zvs-jobd", "--package", "zvs-cli", ...profile],
  {
    cwd: root,
    stdio: "inherit",
  },
);
if (features.includes("e2e")) {
  execFileSync(
    "cargo",
    ["build", "--locked", "--package", "zvs-core", "--example", "seed_vectors", ...profile],
    { cwd: root, stdio: "inherit" },
  );
}

const bundling = release && !rest.includes("--no-bundle");
const sidecar = join(
  root,
  "target/release",
  process.platform === "win32" ? "zvs-jobd.exe" : "zvs-jobd",
);
const cli = join(
  root,
  "target/release",
  process.platform === "win32" ? "aistudio.exe" : "aistudio",
);
const overlay = join(root, "target/tauri.overlay.json");
writeFileSync(
  overlay,
  JSON.stringify({
    version: readAppEnv().ZVS_APP_VERSION,
    ...(bundling
      ? { bundle: { resources: { [sidecar]: basename(sidecar), [cli]: `cli/${basename(cli)}` } } }
      : {}),
  }),
);
const tauri = spawnSync("pnpm", ["exec", "tauri", command, ...rest, "--config", overlay], {
  cwd: join(root, "apps/studio"),
  stdio: "inherit",
  shell: process.platform === "win32",
});
process.exit(tauri.status ?? 1);
