import { randomUUID } from "node:crypto";
import { readdirSync } from "node:fs";
import { createServer, type Server, type ServerResponse } from "node:http";
import { homedir } from "node:os";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { expect, test, type Page } from "@playwright/test";
import { temporaryDirectory, type TemporaryDirectory } from "../test/helpers/paths.ts";
import { launchStudio, type Studio } from "./launch.ts";

interface Report {
  readonly path: string;
  readonly kind: string;
  readonly data: unknown;
}

interface Size {
  readonly width: number;
  readonly height: number;
  readonly visible: string;
}

let app: Studio;
let studio: Page;
let chrome: Page;
let profile: TemporaryDirectory;
let server: Server;
let origin: string;
let port: number;
const oauthRequests: { method: string; body: string }[] = [];
const downloads: string[] = [];
const reports: Report[] = [];
const gates = new Map<string, ServerResponse[]>();
const blockedFile = `zvs-e2e-blocked-${randomUUID()}.txt`;

const FIXTURE_SCRIPT = `
const path = location.pathname + location.search;
const report = (kind, data) =>
  fetch("/report", { method: "POST", keepalive: true, body: JSON.stringify({ path, kind, data }) });
report("globals", {
  zvs: typeof window.zvs,
  require: typeof window.require,
  process: typeof window.process,
  tauri: typeof window.__TAURI_INTERNALS__,
});
Promise.all(
  ["geolocation", "notifications", "camera", "microphone"].map((name) =>
    navigator.permissions.query({ name }).then((status) => status.state),
  ),
).then((states) => report("permissions", states));
navigator.geolocation.getCurrentPosition(
  () => report("geolocation", "allowed"),
  (error) => report("geolocation", error.code),
);
const size = () =>
  report("size", { width: innerWidth, height: innerHeight, visible: document.visibilityState });
size();
addEventListener("resize", size);
document.addEventListener("visibilitychange", size);
window.addEventListener("message", (event) => report("message", event.data));
const action = new URLSearchParams(location.search).get("do");
if (action === "popup") window.open("/popup");
if (action === "download") {
  const link = document.createElement("a");
  link.href = "/download";
  document.body.append(link);
  link.click();
}
if (action === "oauth") {
  const form = document.createElement("form");
  form.method = "POST";
  form.target = "_blank";
  form.rel = "opener";
  form.action = "/oauth";
  const input = document.createElement("input");
  input.name = "state";
  input.value = "test-nonce";
  form.append(input);
  document.body.append(form);
  form.submit();
}
`;

const OAUTH_SCRIPT = `
fetch("/report", {
  method: "POST",
  body: JSON.stringify({
    path: "/oauth",
    kind: "globals",
    data: {
      zvs: typeof window.zvs,
      require: typeof window.require,
      process: typeof window.process,
      tauri: typeof window.__TAURI_INTERNALS__,
    },
  }),
});
window.opener?.postMessage("oauth-ok", location.origin);
fetch("/gate/close-oauth").then(() => window.close());
`;

function reported<T>(path: string, kind: string): T | undefined {
  return reports.findLast((report) => report.path === path && report.kind === kind)?.data as
    T | undefined;
}

function release(gate: string): void {
  for (const response of gates.get(gate) ?? []) response.end("go");
  gates.delete(gate);
}

async function launch(): Promise<void> {
  app = await launchStudio(profile.path);
  studio = app.page;
}

async function openBrowser(): Promise<void> {
  await studio.locator("aside").getByText("Браузер", { exact: true }).click();
  chrome = await app.chrome();
  await expect(chrome.getByRole("button", { name: "Сайты & cookies" })).toBeVisible();
  await expect
    .poll(() => chrome.evaluate(() => innerWidth), { timeout: 30_000 })
    .toBeGreaterThan(100);
}

async function go(url: string): Promise<void> {
  await expect(chrome.getByRole("tab").first()).toBeVisible();
  const address = chrome.getByRole("textbox", { name: "Адрес", exact: true });
  await expect(async () => {
    await address.fill(url);
    await expect(address).toHaveValue(url, { timeout: 500 });
  }).toPass();
  await chrome.getByRole("button", { name: "Перейти", exact: true }).click();
}

async function profileCookies(): Promise<{ domain: string; name: string }[]> {
  return (await chrome.evaluate(() =>
    window.zvs.call("browser.command", { action: "cookies" }),
  )) as { domain: string; name: string }[];
}

async function studioViewport(): Promise<{ width: number; height: number }> {
  return studio.evaluate(() => ({ width: innerWidth, height: innerHeight }));
}

test.beforeAll(async () => {
  profile = temporaryDirectory("browser-e2e-");
  server = createServer((request, response) => {
    const url = request.url ?? "/";
    if (url === "/report") {
      let body = "";
      request.on("data", (chunk: Buffer) => {
        body += chunk.toString();
      });
      request.on("end", () => {
        reports.push(JSON.parse(body) as Report);
        response.end();
      });
      return;
    }
    if (url.startsWith("/gate/")) {
      const gate = url.slice("/gate/".length);
      gates.set(gate, [...(gates.get(gate) ?? []), response]);
      return;
    }
    if (url === "/oauth") {
      let body = "";
      request.on("data", (chunk: Buffer) => {
        body += chunk.toString();
      });
      request.on("end", () => {
        oauthRequests.push({ method: request.method ?? "", body });
        response.writeHead(200, { "Content-Type": "text/html", "Cache-Control": "no-store" });
        response.end(`<title>OAuth callback</title><script>${OAUTH_SCRIPT}</script>`);
      });
      return;
    }
    if (url === "/download") {
      downloads.push(url);
      response.writeHead(200, { "Content-Disposition": `attachment; filename=${blockedFile}` });
      response.end("blocked");
      return;
    }
    response.writeHead(200, {
      "Content-Type": "text/html",
      "Cache-Control": "no-store",
      "Set-Cookie": "login=private-value; Path=/; Max-Age=3600; HttpOnly",
    });
    response.end(
      `<title>${url.startsWith("/second") ? "Second page" : "Test site"}</title><h1>Local browser test</h1><a href="/second">Next</a><script>${FIXTURE_SCRIPT}</script>`,
    );
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("Missing server address");
  port = address.port;
  origin = `http://127.0.0.1:${port}`;
  await launch();
});

test.afterAll(async () => {
  await app?.close();
  for (const gate of [...gates.keys()]) release(gate);
  server?.closeAllConnections();
  await new Promise<void>((resolve) => server?.close(() => resolve()));
  profile?.dispose();
});

test("isolated browser, navigation, cookie management and restart persistence", async () => {
  await openBrowser();
  const studioPages = (): Page[] =>
    app.pages().filter((page) => page.url().includes("tauri.localhost"));
  expect(studioPages()).toHaveLength(2);
  await go(origin);
  await expect(chrome.getByRole("tab", { name: "Test site" })).toBeVisible();
  await expect(chrome.getByText("Подключение не защищено", { exact: true })).toBeVisible();
  await expect.poll(() => app.surfaces()).toEqual({ windows: 1, webviews: 3 });
  await expect
    .poll(() => reported("/", "globals"))
    .toEqual({
      zvs: "undefined",
      require: "undefined",
      process: "undefined",
      tauri: "undefined",
    });
  await expect.poll(() => reported<string[]>("/", "permissions")).toHaveLength(4);
  expect(reported<string[]>("/", "permissions")).not.toContain("granted");
  await expect.poll(() => reported("/", "geolocation")).toBe(1);
  const studioSession = await studio.context().newCDPSession(studio);
  const studioCookies = (await studioSession.send("Network.getCookies", { urls: [origin] }))
    .cookies;
  await studioSession.detach();
  expect(studioCookies).toEqual([]);
  expect(await profileCookies()).toEqual([
    expect.objectContaining({ domain: "127.0.0.1", name: "login" }),
  ]);

  const panel = await studio
    .getByLabel("Browser workspace", { exact: true })
    .evaluate((element) => {
      const { width, height } = element.getBoundingClientRect();
      return { width: Math.round(width), height: Math.round(height) };
    });
  expect(panel.width).toBeLessThan((await studioViewport()).width - 200);
  await expect
    .poll(() => reported<Size>("/", "size"))
    .toEqual({ width: panel.width, height: panel.height - 176, visible: "visible" });
  await studio.locator("aside").getByRole("button", { name: "Свернуть панель навигации" }).click();
  await expect.poll(() => reported<Size>("/", "size")?.width).toBe(panel.width + 180);
  await studio
    .locator("aside")
    .getByRole("button", { name: "Развернуть панель навигации" })
    .click();
  await expect.poll(() => reported<Size>("/", "size")?.width).toBe(panel.width);
  await studio.locator("aside").getByText("Расширения", { exact: true }).click();
  await expect(studio.locator("h1")).toHaveText("Расширения");
  await expect.poll(() => reported<Size>("/", "size")?.visible).toBe("hidden");
  await expect.poll(() => app.surfaces()).toEqual({ windows: 1, webviews: 1 });
  await openBrowser();
  await expect(chrome.getByRole("tab", { name: "Test site" })).toBeVisible();
  await expect.poll(() => reported<Size>("/", "size")?.visible).toBe("visible");

  await go(`${origin}/?do=download`);
  await expect.poll(() => downloads).toHaveLength(1);
  await delay(2_000);
  expect(
    readdirSync(join(homedir(), "Downloads")).filter((name) => name.includes(blockedFile)),
  ).toEqual([]);
  await expect(chrome.getByRole("tab")).toHaveCount(1);

  await go(`${origin}/second`);
  await expect(chrome.getByRole("tab", { name: "Second page" })).toBeVisible();
  await chrome.getByRole("button", { name: "Назад", exact: true }).click();
  await expect(chrome.getByRole("tab", { name: "Test site" })).toBeVisible();
  await chrome.getByRole("button", { name: "Вперед", exact: true }).click();
  await expect(chrome.getByRole("tab", { name: "Second page" })).toBeVisible();
  await go("file:///etc/passwd");
  await expect(chrome.getByRole("status")).toContainText("Action failed");

  await go(`${origin}/second?do=popup`);
  await expect(chrome.getByRole("tab")).toHaveCount(2);
  await expect.poll(() => app.surfaces().windows).toBe(1);
  expect(studioPages()).toHaveLength(2);
  await expect
    .poll(() => reported("/popup", "globals"))
    .toEqual({
      zvs: "undefined",
      require: "undefined",
      process: "undefined",
      tauri: "undefined",
    });
  await chrome.getByRole("tab").last().press("Alt+ArrowLeft");
  await expect(chrome.getByRole("tab").first()).toHaveAttribute("aria-selected", "true");
  await chrome.getByRole("button", { name: "Закрыть Test site", exact: true }).click();
  await expect(chrome.getByRole("tab")).toHaveCount(1);

  await go(`${origin}/second?do=oauth`);
  await expect(chrome.getByRole("tab", { name: "OAuth callback" })).toBeVisible();
  expect(oauthRequests).toEqual([{ method: "POST", body: "state=test-nonce" }]);
  await expect.poll(() => reported("/second?do=oauth", "message")).toBe("oauth-ok");
  await expect
    .poll(() => reported("/oauth", "globals"))
    .toEqual({
      zvs: "undefined",
      require: "undefined",
      process: "undefined",
      tauri: "undefined",
    });
  await expect.poll(() => gates.has("close-oauth")).toBe(true);
  release("close-oauth");
  await expect(chrome.getByRole("tab")).toHaveCount(1);

  await chrome.evaluate(
    (url) => window.zvs.call("browser.command", { action: "new", url }),
    origin.replace("http:", "https:"),
  );
  await expect(chrome.getByRole("status")).toContainText("Page could not load");
  await expect(chrome.getByText("Ошибка загрузки контента", { exact: true })).toBeVisible();
  await chrome
    .locator(".tab")
    .last()
    .getByRole("button", { name: /^Закрыть / })
    .click();
  await expect(chrome.getByRole("tab")).toHaveCount(1);

  await chrome.getByRole("button", { name: "Сайты & cookies" }).click();
  await expect(chrome.getByRole("cell", { name: "login /" })).toBeVisible();
  expect(await chrome.locator("body").innerText()).not.toContain("private-value");
  await chrome.screenshot({ path: "test-results/browser-cookies.png" });
  await expect.poll(() => reported<Size>("/second?do=oauth", "size")?.visible).toBe("hidden");
  await expect.poll(() => app.surfaces()).toEqual({ windows: 1, webviews: 2 });
  const rejected = await chrome.evaluate(async () => {
    try {
      await window.zvs.call("settings.get", { key: "window.main" });
      return false;
    } catch {
      return true;
    }
  });
  expect(rejected).toBe(true);

  await studio.locator("aside").getByText("Расширения", { exact: true }).click();
  await app.close();
  await launch();
  await openBrowser();
  await chrome.getByRole("button", { name: "Сайты & cookies" }).click();
  await expect(chrome.getByRole("cell", { name: "login /" })).toBeVisible();
  await chrome.getByRole("button", { name: "Удалить", exact: true }).click();
  await chrome.getByRole("button", { name: "Confirm removal" }).click();
  await expect(chrome.getByText("Нет куки.", { exact: false })).toBeVisible();
  expect(await profileCookies()).toEqual([]);

  await go(`http://localhost:${port}/`);
  await expect(chrome.getByRole("tab", { name: "Test site" })).toBeVisible();
  await go(`${origin}/`);
  await expect.poll(async () => (await profileCookies()).length).toBe(2);
  await chrome.getByRole("button", { name: "Сайты & cookies" }).click();
  await chrome.getByRole("button", { name: "Обновить", exact: true }).click();
  await chrome
    .locator("article")
    .filter({ has: chrome.getByRole("heading", { name: "127.0.0.1", exact: true }) })
    .getByRole("button", { name: "Очистить cookies" })
    .click();
  await chrome.getByRole("button", { name: "Confirm removal" }).click();
  await expect(chrome.getByRole("heading", { name: "127.0.0.1", exact: true })).toHaveCount(0);
  await expect(chrome.getByRole("heading", { name: "localhost", exact: true })).toBeVisible();
});
