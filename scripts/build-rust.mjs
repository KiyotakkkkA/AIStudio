import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, URL } from "node:url";
import process from "node:process";
import { NapiCli } from "@napi-rs/cli";

const root = fileURLToPath(new URL("../", import.meta.url));
const localProtoc = join(
  root,
  "target/tools/protoc/bin",
  process.platform === "win32" ? "protoc.exe" : "protoc",
);
if (!process.env.PROTOC && existsSync(localProtoc)) process.env.PROTOC = localProtoc;
const debug = process.argv.includes("--debug-panic");
const target = execFileSync("rustc", ["-vV"], { encoding: "utf8", cwd: root })
  .match(/^host: (.+)$/m)?.[1]
  ?.trim();
if (!target) throw new Error("rustc did not report a host target");
const outputDir = join(root, "apps/studio/resources/native", target, ...(debug ? ["debug"] : []));
const build = await new NapiCli().build({
  cwd: join(root, "crates/zvs-napi"),
  platform: true,
  release: true,
  target,
  outputDir,
  noJsBinding: true,
  noDtsHeader: true,
  ...(debug ? { features: ["debug-panic"] } : {}),
  cargoOptions: ["--locked"],
});
const outputs = await build.task;
const addon = outputs.find((output) => output.kind === "node");
if (!addon) throw new Error("napi build did not produce a .node artifact");
copyFileSync(addon.path, join(outputDir, "zvs-core.node"));

// The sidecar is a plain binary, packaged as an extraResource and located through paths.ts.
// It has no debug variant: the debug build exists only for the addon's panic tests.
if (!debug) {
  execFileSync("cargo", ["build", "--release", "--locked", "--package", "zvs-jobd"], {
    cwd: root,
    stdio: "inherit",
  });
  const binary = process.platform === "win32" ? "zvs-jobd.exe" : "zvs-jobd";
  const sidecarDir = join(root, "apps/studio/resources/sidecar", target);
  mkdirSync(sidecarDir, { recursive: true });
  copyFileSync(join(root, "target/release", binary), join(sidecarDir, binary));
}
