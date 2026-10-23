import { setTimeout as delay } from "node:timers/promises";
import { expect, test, type Page } from "@playwright/test";
import { temporaryDirectory, type TemporaryDirectory } from "../test/helpers/paths.ts";
import {
  freePort,
  launchStudio,
  listening,
  readLog,
  startStudio,
  stopStudio,
  type Studio,
} from "./launch.ts";

/** All fifteen rail entries stay within the studio window. */
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
  "Расширения",
];

const ROUTE_HEADERS = [
  { item: "Секреты", header: "Секреты" },
  { item: "Навыки", header: "Навыки" },
  { item: "Расширения", header: "Расширения" },
];

let studio: Studio;
let page: Page;
let userData: TemporaryDirectory;
const problems: string[] = [];

test.beforeAll(async () => {
  userData = temporaryDirectory("studio-e2e-");
  studio = await launchStudio(userData.path);
  page = studio.page;
  page.on("console", (message) => {
    if (message.type() === "error") problems.push(`console: ${message.text()}`);
  });
  page.on("pageerror", (error) => problems.push(`pageerror: ${error.message}`));
});

test.afterAll(async () => {
  await studio?.close();
  userData?.dispose();
});

test("renderer calls the real Rust chunker through nativePing", async () => {
  const result = await page.evaluate(() =>
    window.zvs.call("system.nativePing", { text: "word ".repeat(600) }),
  );
  expect(result).toEqual({ ok: true, data: { count: 3 } });
});

test("the built app opens a window with the whole navigation rail", async () => {
  await expect(page.locator("#root")).toBeVisible();
  expect(studio.pages().filter((opened) => opened.url().includes("tauri.localhost"))).toHaveLength(
    1,
  );
  expect(studio.surfaces()).toEqual({ windows: 1, webviews: 1 });
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

test("startup applies every migration exactly once", () => {
  const applied = readLog(userData.path)
    .filter((line) => line.message === "Applied migration")
    .map((line) => line.migration);
  expect(applied).toEqual([
    "0000_baseline",
    "0001_graph_relation_seed",
    "0002_remove_deepseek_web",
  ]);
});

test("every rail page renders under the Tauri shell without an error boundary", async () => {
  for (const label of RAIL_ITEMS) {
    await page.locator("aside nav").getByText(label, { exact: true }).click();
    await expect(page.locator("main")).toBeVisible();
    await expect(page.getByText("Интерфейс не удалось отрисовать", { exact: true })).toHaveCount(0);
  }
  await page.locator("aside").getByText("Расширения", { exact: true }).click();
});

test("every channel group answers from the Rust host", async () => {
  const calls: [string, unknown][] = [
    ["accounts.list", undefined],
    ["chat.conversations.list", undefined],
    ["downloads.list", {}],
    ["graphRelations.list", undefined],
    ["mcp.list", undefined],
    ["providers.list", {}],
    ["providers.adapters", undefined],
    ["runs.list", {}],
    ["runtimes.list", undefined],
    ["secrets.list", {}],
    ["settings.get", { key: "ui.theme" }],
    ["system.device", { refresh: false }],
    ["tools.list", {}],
    ["vectorStores.list", undefined],
  ];
  const failed: string[] = [];
  for (const [channel, payload] of calls) {
    const result = (await page.evaluate(
      ({ channel, payload }) => window.zvs.call(channel, payload),
      { channel, payload },
    )) as { ok: boolean };
    if (!result.ok) failed.push(`${channel}: ${JSON.stringify(result)}`);
  }
  expect(failed).toEqual([]);
});

test("system.ping round-trips from the renderer to the host", async () => {
  await page.locator("aside").getByText("Расширения", { exact: true }).click();
  const value = page.getByTestId("ping-value");
  await expect(value).toHaveText("—");
  await page.getByTestId("ping-button").click();
  await expect(value).toHaveText(/^pong · \d+$/);
});

test("the demo stream counts to completion over the event channel", async () => {
  await page.locator("aside").getByText("Расширения", { exact: true }).click();
  const value = page.getByTestId("stream-value");
  await page.getByTestId("stream-button").click();
  await expect(value).toHaveText("1/5");
  await expect(value).toHaveText("готово · ok");
});

test("the secret channels answer the renderer without ever carrying a value", async () => {
  const types = await page.evaluate(() => window.zvs.call("secrets.types", undefined));
  expect(types).toMatchObject({ ok: true });
  expect((types as { data: { key: string }[] }).data.map((schema) => schema.key)).toEqual([
    "ollama",
    "openrouter",
    "mistral",
    "custom",
  ]);

  const VALUE = "osk_live_0a1b2c3d4e5f60718293a4b5c6d7e8f4f2a";
  const created = await page.evaluate(
    (value) =>
      window.zvs.call("secrets.create", {
        type: "ollama",
        name: "Ollama — e2e",
        scope: "personal",
        value,
        fields: { baseUrl: "https://ollama.com/api" },
        tags: ["e2e"],
      }),
    VALUE,
  );
  expect(created).toMatchObject({ ok: true });

  const listed = await page.evaluate(() => window.zvs.call("secrets.list", {}));
  expect(listed).toMatchObject({ ok: true, data: [{ hint: "osk_live_…4f2a" }] });
  expect(JSON.stringify(listed)).not.toContain(VALUE);

  const rejected = await page.evaluate(
    (value) =>
      window.zvs.call("secrets.create", {
        type: "ollama",
        name: "Ollama — sneaky",
        scope: "personal",
        value,
        fields: { sneaky: "value" },
      }),
    VALUE,
  );
  expect(rejected).toMatchObject({ ok: false, error: { code: "VALIDATION_FAILED" } });
});

test("nothing logged a console error or a content security policy violation", () => {
  expect(problems, problems.join("\n")).toEqual([]);
});

test("a WEBVIEW2_* variable from the environment never opens a debugging port", async () => {
  const directory = temporaryDirectory("studio-e2e-hardening-");
  const port = await freePort();
  const child = startStudio(directory.path, {
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  });
  try {
    await expect
      .poll(
        () => readLog(directory.path).some((line) => line.message === "Opened the event channel"),
        {
          timeout: 60_000,
        },
      )
      .toBe(true);
    for (let attempt = 0; attempt < 20; attempt += 1) {
      expect(await listening(port)).toBe(false);
      await delay(500);
    }
    expect(child.exitCode).toBeNull();
  } finally {
    await stopStudio(child);
    directory.dispose();
  }
});
