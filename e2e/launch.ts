import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { createServer } from "node:net";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { chromium, expect, type Browser, type Page } from "@playwright/test";
import { REPO_ROOT, assertTemporaryPath } from "../test/helpers/paths.ts";

declare global {
  interface Window {
    readonly zvs: { call(channel: string, payload: unknown): Promise<unknown> };
  }
}

export const STUDIO_EXECUTABLE = join(REPO_ROOT, "target", "release", "zvs-studio.exe");
const STUDIO_HOST = "tauri.localhost";
const CHROME_PATH = "/browser-ui/";
const STARTUP_TIMEOUT = 60_000;
const CLOSE_TIMEOUT = 20_000;
const ROUTE_SETTLE = 750;

export default function buildStudioOnce(): void {
  if (process.env.ZVS_E2E_PREBUILT === "1") return;
  const build = spawnSync(
    process.execPath,
    ["scripts/tauri.mjs", "build", "--no-bundle", "--features", "e2e"],
    { cwd: REPO_ROOT, stdio: "inherit" },
  );
  if (build.status !== 0) throw new Error("The e2e Tauri build failed");
}

export async function freePort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  await new Promise<void>((resolve) => server.close(() => resolve()));
  if (!address || typeof address === "string") throw new Error("No free port");
  return address.port;
}

export async function listening(port: number): Promise<boolean> {
  try {
    const response = await fetch(`http://127.0.0.1:${port}/json/version`, {
      signal: AbortSignal.timeout(1_000),
    });
    return response.ok;
  } catch {
    return false;
  }
}

function isStudioPage(page: Page): boolean {
  const url = URL.parse(page.url());
  return url?.hostname === STUDIO_HOST && !url.pathname.startsWith(CHROME_PATH);
}

function isChromePage(page: Page): boolean {
  const url = URL.parse(page.url());
  return url?.hostname === STUDIO_HOST && url.pathname.startsWith(CHROME_PATH);
}

export function startStudio(dataDir: string, env: NodeJS.ProcessEnv = {}): ChildProcess {
  expect(
    existsSync(STUDIO_EXECUTABLE),
    `${STUDIO_EXECUTABLE} is missing — run \`pnpm build:tauri -- --features e2e\``,
  ).toBe(true);
  const environment: NodeJS.ProcessEnv = { ...process.env, ...env };
  environment.ZVS_DATA_DIR = assertTemporaryPath(dataDir);
  return spawn(STUDIO_EXECUTABLE, [], {
    cwd: join(REPO_ROOT, "target", "release"),
    env: environment,
    stdio: "ignore",
    windowsHide: false,
  });
}

function exited(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return Promise.resolve();
  return new Promise((resolve) => child.once("exit", () => resolve()));
}

export async function stopStudio(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null || child.pid === undefined) return;
  spawnSync("taskkill", ["/pid", String(child.pid)], { stdio: "ignore" });
  const graceful = await Promise.race([
    exited(child).then(() => true),
    delay(CLOSE_TIMEOUT).then(() => false),
  ]);
  if (!graceful) {
    spawnSync("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore" });
    await exited(child);
  }
}

export function readLog(dataDir: string): { message: string; [field: string]: unknown }[] {
  const file = join(dataDir, "logs", "studio.jsonl");
  if (!existsSync(file)) return [];
  return readFileSync(file, "utf8")
    .trim()
    .split("\n")
    .filter((line) => line.length > 0)
    .map((line) => JSON.parse(line) as { message: string });
}

async function settled(page: Page): Promise<void> {
  let route = page.url();
  let since = Date.now();
  const deadline = since + STARTUP_TIMEOUT;
  while (Date.now() - since < ROUTE_SETTLE) {
    if (Date.now() > deadline) throw new Error("The studio route never settled");
    await delay(50);
    const current = page.url();
    if (current !== route) {
      route = current;
      since = Date.now();
    }
  }
}

export interface Surfaces {
  readonly windows: number;
  readonly webviews: number;
}

function surfaces(pid: number): Surfaces {
  const result = spawnSync(
    "powershell",
    ["-NoProfile", "-NonInteractive", "-File", join(REPO_ROOT, "e2e", "windows.ps1"), String(pid)],
    { encoding: "utf8" },
  );
  return JSON.parse(result.stdout.trim()) as Surfaces;
}

export interface Studio {
  readonly browser: Browser;
  readonly page: Page;
  readonly dataDir: string;
  pages(): Page[];
  surfaces(): Surfaces;
  chrome(): Promise<Page>;
  close(): Promise<void>;
}

export async function launchStudio(dataDir: string): Promise<Studio> {
  const port = await freePort();
  const child = startStudio(dataDir, { ZVS_E2E_CDP_PORT: String(port) });
  try {
    const deadline = Date.now() + STARTUP_TIMEOUT;
    while (!(await listening(port))) {
      if (child.exitCode !== null) throw new Error(`The studio exited with ${child.exitCode}`);
      if (Date.now() > deadline)
        throw new Error(`No CDP endpoint on ${port}: is this an e2e build?`);
      await delay(250);
    }
    const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
    const pages = (): Page[] => browser.contexts().flatMap((context) => context.pages());
    await expect.poll(() => pages().some(isStudioPage), { timeout: STARTUP_TIMEOUT }).toBe(true);
    const page = pages().find(isStudioPage)!;
    await page.waitForLoadState("domcontentloaded");
    await page.locator("#root > *").first().waitFor({ timeout: STARTUP_TIMEOUT });
    await settled(page);
    return {
      browser,
      page,
      dataDir,
      pages,
      surfaces: () => surfaces(child.pid!),
      async chrome(): Promise<Page> {
        await expect.poll(() => pages().some(isChromePage)).toBe(true);
        const chrome = pages().find(isChromePage)!;
        await chrome.waitForLoadState("domcontentloaded");
        return chrome;
      },
      async close(): Promise<void> {
        await stopStudio(child);
        await browser.close().catch(() => undefined);
      },
    };
  } catch (error) {
    await stopStudio(child);
    throw error;
  }
}

export async function rpc<T>(page: Page, channel: string, payload?: unknown): Promise<T> {
  const result = (await page.evaluate(({ channel, payload }) => window.zvs.call(channel, payload), {
    channel,
    payload,
  })) as { ok: boolean; data: T };
  expect(result).toMatchObject({ ok: true });
  return result.data;
}
