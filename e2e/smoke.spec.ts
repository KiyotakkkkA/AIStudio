import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import {
  _electron as electron,
  expect,
  test,
  type ElectronApplication,
  type Page,
} from "@playwright/test";
import { temporaryDirectory, type TemporaryDirectory } from "../test/helpers/paths.ts";

const STUDIO_ROOT = fileURLToPath(new URL("../apps/studio", import.meta.url));
const MAIN_BUNDLE = join(STUDIO_ROOT, "out", "host", "main.js");

/** The rail is fourteen routes plus the Browser entry, which opens a window instead. */
const RAIL_ITEMS = [
  "Чат",
  "Агенты",
  "Провадеры",
  "Сценарии",
  "Секреты",
  "Векторные хранилища",
  "Навыки",
  "Коннекторы",
  "Интеграции",
  "Инструменты",
  "Браузер",
  "Задачи",
  "Загрузки",
  "Запуски и логи",
  "Настройки",
];

const ROUTE_HEADERS = [
  { item: "Секреты", header: "Секреты" },
  { item: "Навыки", header: "Навыки" },
  { item: "Настройки", header: "Настройки" },
];

/** Strips the dev-server URL so the host loads the built bundle, exactly as it ships. */
function productionEnvironment(): NodeJS.ProcessEnv {
  const environment = { ...process.env };
  delete environment.ELECTRON_RENDERER_URL;
  return environment;
}

const electronExecutable = (): string =>
  createRequire(pathToFileURL(join(STUDIO_ROOT, "package.json")).href)("electron") as string;

let app: ElectronApplication;
let page: Page;
let userData: TemporaryDirectory;
const problems: string[] = [];

test.beforeAll(async () => {
  expect(
    existsSync(MAIN_BUNDLE),
    `${MAIN_BUNDLE} is missing — run \`pnpm build\` before \`pnpm test:e2e\``,
  ).toBe(true);

  userData = temporaryDirectory("studio-e2e-");
  app = await electron.launch({
    executablePath: electronExecutable(),
    cwd: STUDIO_ROOT,
    args: [STUDIO_ROOT, `--user-data-dir=${userData.path}`],
    env: { ...productionEnvironment(), NODE_ENV: "production" },
  });

  const watch = (opened: Page): void => {
    opened.on("console", (message) => {
      if (message.type() === "error") problems.push(`console: ${message.text()}`);
    });
    opened.on("pageerror", (error) => problems.push(`pageerror: ${error.message}`));
  };
  app.on("window", watch);
  page = await app.firstWindow();
  watch(page);
  await page.waitForLoadState("domcontentloaded");
});

test.afterAll(async () => {
  await app?.close();
  userData?.dispose();
});

test("the built app opens a window with the whole navigation rail", async () => {
  await expect(page.locator("#root")).toBeVisible();
  expect(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().length)).toBe(1);
  expect(await page.title()).toBe("ZVS AI Studio");

  const rail = page.locator("aside");
  await expect(rail).toBeVisible();
  for (const label of RAIL_ITEMS) {
    await expect(rail.getByText(label, { exact: true })).toHaveCount(1);
  }
  await expect(rail.locator("nav button")).toHaveCount(RAIL_ITEMS.length);
});

test("the rail routes between pages and each page renders its own header", async () => {
  for (const { item, header } of ROUTE_HEADERS) {
    await page.locator("aside").getByText(item, { exact: true }).click();
    await expect(page.locator("h1")).toHaveText(header);
  }
});

test("system.ping round-trips from the renderer to the host", async () => {
  await page.locator("aside").getByText("Настройки", { exact: true }).click();
  const value = page.getByTestId("ping-value");
  await expect(value).toHaveText("—");
  await page.getByTestId("ping-button").click();
  await expect(value).toHaveText(/^pong · \d+$/);
});

test("the demo stream counts to completion over the event channel", async () => {
  await page.locator("aside").getByText("Настройки", { exact: true }).click();
  const value = page.getByTestId("stream-value");
  await page.getByTestId("stream-button").click();
  await expect(value).toHaveText("1/5");
  await expect(value).toHaveText("готово · ok");
});

test("nothing logged a console error or a content security policy violation", () => {
  expect(problems, problems.join("\n")).toEqual([]);
});
