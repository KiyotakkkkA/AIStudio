import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import process from "node:process";

const electron = createRequire(import.meta.url)("electron");
const files = process.argv.slice(2);
const result = spawnSync(
  electron,
  ["--experimental-strip-types", "--test", ...(files.length > 0 ? files : ["tests/*.test.ts"])],
  { stdio: "inherit", env: { ...process.env, ELECTRON_RUN_AS_NODE: "1" } },
);

process.exit(result.status ?? 1);
