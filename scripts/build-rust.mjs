import { execFileSync } from "node:child_process";
import { copyFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, URL } from "node:url";
import process from "node:process";
import { NapiCli } from "@napi-rs/cli";

const root = fileURLToPath(new URL("../", import.meta.url));
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
