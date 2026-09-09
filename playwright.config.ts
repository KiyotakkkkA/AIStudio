import { defineConfig } from "@playwright/test";

/**
 * The e2e suite drives the built application from `apps/studio/out`, never the dev
 * server, so it exercises the bundle, the preload path and the production CSP.
 */
export default defineConfig({
  testDir: "e2e",
  timeout: 60_000,
  expect: { timeout: 15_000 },
  fullyParallel: false,
  workers: 1,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : [["list"]],
});
