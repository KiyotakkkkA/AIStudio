import { createServer, type Server } from "node:http";
import { createRequire } from "node:module";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  _electron as electron,
  expect,
  test,
  type ElectronApplication,
  type Page,
} from "@playwright/test";
import { temporaryDirectory, type TemporaryDirectory } from "../test/helpers/paths.ts";

const root = fileURLToPath(new URL("../apps/studio", import.meta.url));
let app: ElectronApplication;
let studio: Page;
let chrome: Page;
let profile: TemporaryDirectory;
let server: Server;
let origin: string;
const oauthRequests: { method: string; body: string }[] = [];

async function launch() {
  const env = { ...process.env, NODE_ENV: "production" };
  delete (env as NodeJS.ProcessEnv).ELECTRON_RENDERER_URL;
  app = await electron.launch({
    executablePath: createRequire(join(root, "package.json"))("electron") as string,
    cwd: root,
    args: [root, `--user-data-dir=${profile.path}`],
    env,
  });
  studio = await app.firstWindow();
  await studio.waitForLoadState("domcontentloaded");
}

async function openBrowser() {
  await studio.locator("aside").getByText("Браузер", { exact: true }).click();
  await expect
    .poll(() =>
      app
        .context()
        .pages()
        .some((page) => page.url().includes("browser-ui/index.html")),
    )
    .toBe(true);
  chrome = app
    .context()
    .pages()
    .find((page) => page.url().includes("browser-ui/index.html"))!;
  await expect(chrome.getByRole("button", { name: "Сайты & cookies" })).toBeVisible();
}

test.beforeAll(async () => {
  profile = temporaryDirectory("browser-e2e-");
  server = createServer((request, response) => {
    if (request.url === "/oauth") {
      let body = "";
      request.on("data", (chunk: Buffer) => {
        body += chunk.toString();
      });
      request.on("end", () => {
        oauthRequests.push({ method: request.method ?? "", body });
        response.writeHead(200, { "Content-Type": "text/html", "Cache-Control": "no-store" });
        response.end(
          '<title>OAuth callback</title><script>window.opener?.postMessage("oauth-ok", location.origin)</script>',
        );
      });
      return;
    }
    if (request.url === "/download") {
      response.writeHead(200, { "Content-Disposition": "attachment; filename=blocked.txt" });
      response.end("blocked");
      return;
    }
    response.writeHead(200, {
      "Content-Type": "text/html",
      "Set-Cookie": "login=private-value; Path=/; Max-Age=3600; HttpOnly",
    });
    response.end(
      `<title>${request.url === "/second" ? "Second page" : "Test site"}</title><h1>Local browser test</h1><a href="/second">Next</a>`,
    );
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("Missing server address");
  origin = `http://127.0.0.1:${address.port}`;
  await launch();
});

test.afterAll(async () => {
  await app?.close();
  await new Promise<void>((resolve) => server?.close(() => resolve()));
  profile?.dispose();
});

test("isolated browser, navigation, cookie management and restart persistence", async () => {
  await openBrowser();
  expect(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().length)).toBe(1);
  await chrome.getByRole("textbox", { name: "Адрес", exact: true }).fill(origin);
  await chrome.getByRole("button", { name: "Перейти", exact: true }).click();
  await expect(chrome.getByRole("tab", { name: "Test site" })).toBeVisible();
  await expect(chrome.getByText("Подключение не защищено", { exact: true })).toBeVisible();
  const isolation = await app.evaluate(async ({ webContents, session }, origin) => {
    const page = webContents.getAllWebContents().find((item) => item.getURL() === `${origin}/`)!;
    const globals = await page.executeJavaScript(
      "({ zvs: typeof window.zvs, require: typeof require, process: typeof process })",
    );
    const permissions = await page.executeJavaScript(
      "Promise.all(['geolocation','notifications','camera','microphone'].map(name => navigator.permissions.query({name}).then(p => p.state)))",
    );
    return {
      globals,
      permissions,
      separate: page.session !== session.defaultSession,
      defaultCookies: await session.defaultSession.cookies.get({ url: origin }),
      profileCookies: (await page.session.cookies.get({ url: origin })).length,
    };
  }, origin);
  expect(isolation.globals).toEqual({
    zvs: "undefined",
    require: "undefined",
    process: "undefined",
  });
  expect(isolation.permissions).toEqual(["denied", "denied", "denied", "denied"]);
  expect(isolation.separate).toBe(true);
  expect(isolation.defaultCookies).toEqual([]);
  expect(isolation.profileCookies).toBe(1);
  const deniedRequest = await app.evaluate(async ({ webContents }, origin) => {
    const page = webContents.getAllWebContents().find((item) => item.getURL() === `${origin}/`)!;
    return page.executeJavaScript(
      "new Promise(resolve => navigator.geolocation.getCurrentPosition(() => resolve('allowed'), error => resolve(error.code)))",
    );
  }, origin);
  expect(deniedRequest).toBe(1);
  const geometry = await app.evaluate(({ BrowserWindow }) => {
    const window = BrowserWindow.getAllWindows()[0]!;
    const [width, height] = window.getContentSize();
    return { bounds: window.contentView.children[1]!.getBounds(), width, height };
  });
  expect(geometry.bounds).toEqual({
    x: 248,
    y: 136,
    width: geometry.width! - 248,
    height: geometry.height! - 164,
  });
  await studio.locator("aside").getByRole("button", { name: "Свернуть панель навигации" }).click();
  await expect
    .poll(() =>
      app.evaluate(
        ({ BrowserWindow }) =>
          BrowserWindow.getAllWindows()[0]!.contentView.children[1]!.getBounds().x,
      ),
    )
    .toBe(64);
  await studio
    .locator("aside")
    .getByRole("button", { name: "Развернуть панель навигации" })
    .click();
  await expect
    .poll(() =>
      app.evaluate(
        ({ BrowserWindow }) =>
          BrowserWindow.getAllWindows()[0]!.contentView.children[1]!.getBounds().x,
      ),
    )
    .toBe(248);
  await studio.locator("aside").getByText("Настройки", { exact: true }).click();
  await expect(studio.locator("h1")).toHaveText("Настройки");
  await expect
    .poll(() =>
      app.evaluate(
        ({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.contentView.children.length,
      ),
    )
    .toBe(0);
  await openBrowser();
  await expect(chrome.getByRole("tab", { name: "Test site" })).toBeVisible();
  const downloadBlocked = await app.evaluate(async ({ session, webContents }, origin) => {
    const profile = session.fromPartition("persist:browser-work");
    const result = new Promise<boolean>((resolve) =>
      profile.once("will-download", (event) => resolve(event.defaultPrevented)),
    );
    webContents
      .getAllWebContents()
      .find((item) => item.getURL() === `${origin}/`)!
      .downloadURL(`${origin}/download`);
    return result;
  }, origin);
  expect(downloadBlocked).toBe(true);

  await chrome.getByRole("textbox", { name: "Адрес", exact: true }).fill(`${origin}/second`);
  await chrome.getByRole("button", { name: "Перейти", exact: true }).click();
  await expect(chrome.getByRole("tab", { name: "Second page" })).toBeVisible();
  await chrome.getByRole("button", { name: "Назад", exact: true }).click();
  await expect(chrome.getByRole("tab", { name: "Test site" })).toBeVisible();
  await chrome.getByRole("button", { name: "Вперед", exact: true }).click();
  await expect(chrome.getByRole("tab", { name: "Second page" })).toBeVisible();
  await chrome.getByRole("textbox", { name: "Адрес", exact: true }).fill("file:///etc/passwd");
  await chrome.getByRole("button", { name: "Перейти", exact: true }).click();
  await expect(chrome.getByRole("status")).toContainText("Action failed");

  await app.evaluate(async ({ webContents }, origin) => {
    const page = webContents
      .getAllWebContents()
      .find((item) => item.getURL() === `${origin}/second`)!;
    await page.executeJavaScript(`void window.open(${JSON.stringify(`${origin}/popup`)})`);
  }, origin);
  await expect(chrome.getByRole("tab")).toHaveCount(2);
  expect(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().length)).toBe(1);
  await chrome.getByRole("tab").last().press("Alt+ArrowLeft");
  await expect(chrome.getByRole("tab").first()).toHaveAttribute("aria-selected", "true");
  await chrome.getByRole("button", { name: "Закрыть Test site", exact: true }).click();
  await expect(chrome.getByRole("tab")).toHaveCount(1);
  await app.evaluate(async ({ webContents }, origin) => {
    const page = webContents
      .getAllWebContents()
      .find((item) => item.getURL() === `${origin}/second`)!;
    await page.executeJavaScript(`
      window.addEventListener('message', event => { window.oauthResult = event.data; });
      const form = document.createElement('form');
      form.method = 'POST'; form.target = '_blank'; form.rel = 'opener'; form.action = '/oauth';
      const input = document.createElement('input'); input.name = 'state'; input.value = 'test-nonce';
      form.append(input); document.body.append(form); form.submit();
    `);
  }, origin);
  await expect(chrome.getByRole("tab", { name: "OAuth callback" })).toBeVisible();
  expect(oauthRequests).toEqual([{ method: "POST", body: "state=test-nonce" }]);
  await expect
    .poll(() =>
      app.evaluate(async ({ webContents }, origin) => {
        const page = webContents
          .getAllWebContents()
          .find((item) => item.getURL() === `${origin}/second`)!;
        return page.executeJavaScript("window.oauthResult");
      }, origin),
    )
    .toBe("oauth-ok");
  expect(
    await app.evaluate(async ({ webContents }, origin) => {
      const popup = webContents
        .getAllWebContents()
        .find((item) => item.getURL() === `${origin}/oauth`)!;
      return popup.executeJavaScript(
        "({zvs: typeof window.zvs, require: typeof require, process: typeof process})",
      );
    }, origin),
  ).toEqual({ zvs: "undefined", require: "undefined", process: "undefined" });
  await app.evaluate(async ({ webContents }, origin) => {
    const popup = webContents
      .getAllWebContents()
      .find((item) => item.getURL() === `${origin}/oauth`)!;
    await popup.executeJavaScript("setTimeout(() => window.close(), 0)");
  }, origin);
  await expect(chrome.getByRole("tab")).toHaveCount(1);
  // A TLS handshake against a plain HTTP endpoint must never show a secure indicator.
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
  expect(
    await app.evaluate(
      ({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.contentView.children.length,
    ),
  ).toBe(1);
  const rejected = await chrome.evaluate(async () => {
    try {
      await window.zvs.call("settings.get", { key: "window.main" });
      return false;
    } catch {
      return true;
    }
  });
  expect(rejected).toBe(true);
  // The same disk profile is used after a complete Electron restart.
  await studio.locator("aside").getByText("Настройки", { exact: true }).click();
  await app.close();
  await launch();
  await openBrowser();
  await chrome.getByRole("button", { name: "Сайты & cookies" }).click();
  await expect(chrome.getByRole("cell", { name: "login /" })).toBeVisible();
  await chrome.getByRole("button", { name: "Удалить", exact: true }).click();
  await chrome.getByRole("button", { name: "Confirm removal" }).click();
  await expect(chrome.getByText("Нет куки.", { exact: false })).toBeVisible();
  expect(
    await app.evaluate(
      async ({ session }) =>
        (await session.fromPartition("persist:browser-work").cookies.get({})).length,
    ),
  ).toBe(0);
  await app.evaluate(async ({ session }) => {
    const cookies = session.fromPartition("persist:browser-work").cookies;
    await cookies.set({ url: "https://one.example/", name: "one", value: "private" });
    await cookies.set({ url: "https://two.example/", name: "two", value: "private" });
  });
  await chrome.getByRole("button", { name: "Обновить", exact: true }).click();
  await chrome
    .locator("article")
    .filter({ has: chrome.getByRole("heading", { name: "one.example", exact: true }) })
    .getByRole("button", { name: "Очистить cookies" })
    .click();
  await chrome.getByRole("button", { name: "Confirm removal" }).click();
  await expect(chrome.getByRole("heading", { name: "one.example", exact: true })).toHaveCount(0);
  await expect(chrome.getByRole("heading", { name: "two.example", exact: true })).toBeVisible();
});
