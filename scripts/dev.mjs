import { spawnSync } from "node:child_process";
import process from "node:process";
import { DEV_APP_VAR, takeDevApp } from "./dev-app.mjs";

const { app, rest } = takeDevApp(process.argv.slice(2).filter((argument) => argument !== "--"));
if (app !== undefined) process.env[DEV_APP_VAR] = app;
const turbo = spawnSync("pnpm", ["exec", "turbo", "run", "dev", ...rest], {
  stdio: "inherit",
  shell: process.platform === "win32",
});
process.exit(turbo.status ?? 1);
