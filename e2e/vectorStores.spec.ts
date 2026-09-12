import { createServer } from "node:http";
import { createRequire } from "node:module";
import { join } from "node:path";
import { rm } from "node:fs/promises";
import { _electron as electron, expect, test } from "@playwright/test";
import { STUDIO_ROOT, temporaryDirectory, assertTemporaryPath } from "../test/helpers/paths.ts";
import { RustCore } from "../apps/studio/src/host/drivers/rust/RustCore.ts";
import { nativeTargetTriple } from "../apps/studio/src/host/platform/paths.ts";

test("vector stores UI creates, searches native data, edits, detects loss and confirms deletion", async ({
  browserName,
}, testInfo) => {
  expect(browserName).toBe("chromium");
  const directory = temporaryDirectory("studio-vector-ui-");
  const requests: string[] = [];
  const server = createServer(async (request, response) => {
    let body = "";
    for await (const chunk of request) body += String(chunk);
    requests.push(body);
    response.setHeader("Content-Type", "application/json");
    response.end(
      JSON.stringify({
        embeddings: [[1, 0, 0]],
        embedding: [1, 0, 0],
        data: [{ embedding: [1, 0, 0], index: 0 }],
      }),
    );
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("No server address");
  const environment: NodeJS.ProcessEnv = { ...process.env, NODE_ENV: "production" };
  delete environment.ELECTRON_RENDERER_URL;
  const app = await electron.launch({
    executablePath: createRequire(join(STUDIO_ROOT, "package.json"))("electron") as string,
    cwd: STUDIO_ROOT,
    args: [STUDIO_ROOT, `--user-data-dir=${directory.path}`],
    env: { ...environment, NODE_ENV: "production" },
  });
  try {
    const page = await app.firstWindow();
    await app.evaluate(({ BrowserWindow }) => {
      const window = BrowserWindow.getAllWindows()[0]!;
      window.unmaximize();
      window.setContentSize(1440, 900);
    });
    await page.setViewportSize({ width: 1440, height: 900 });
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    const provider = await page.evaluate(
      async (port) =>
        window.zvs.call("providers.create", {
          name: "UI embedding fixture",
          kind: "ollama",
          adapter: "openai-compatible",
          authMode: "api",
          baseUrl: `http://127.0.0.1:${port}`,
          capabilities: ["embedding"],
          enabled: true,
        }),
      address.port,
    );
    expect(provider).toMatchObject({ ok: true });
    await page.locator("nav").getByText("Векторные хранилища", { exact: true }).click();
    await expect(page.getByText("Хранилищ пока нет.", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Новое хранилище", exact: true }).click();
    await page.locator("#vs-name").fill("Product knowledge base");
    await page
      .locator("#vs-description")
      .fill("Manuals, changelogs and FAQ for the ZVS toolchain.");
    await page.locator("#vs-embeddingModelId").fill("fixture-embedding");
    await page.locator("#vs-dimension").fill("3");
    await page.getByText("Выберите провайдер", { exact: true }).click();
    await page.getByText("UI embedding fixture", { exact: true }).click();
    await page.getByRole("button", { name: "Сохранить", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Product knowledge base" })).toBeVisible();
    await expect(page.getByText("Ожидает индексации.", { exact: false })).toBeVisible();
    const response = (await page.evaluate(() =>
      window.zvs.call("vectorStores.list", undefined),
    )) as { ok: boolean; data: { id: string }[] };
    const id = response.data[0]!.id;
    const path = assertTemporaryPath(join(directory.path, "vectors", id));
    const core = RustCore.fromPaths({
      nativeAddonPath: join(STUDIO_ROOT, "resources/native", nativeTargetTriple(), "zvs-core.node"),
    });
    await core.upsertVectors(path, [
      {
        id: "a",
        vector: [1, 0, 0],
        documentId: "manual",
        chunkIndex: 0,
        path: "secrets/rotation.md#L42",
        payload: { text: "Rotating a token replaces the stored value in place.", chunkCount: 2 },
      },
      {
        id: "b",
        vector: [-1, 0, 0],
        documentId: "manual",
        chunkIndex: 1,
        path: "faq/general.md#L96",
        payload: { text: "Secrets are stored per workspace.", chunkCount: 2 },
      },
    ]);
    await page.getByRole("button", { name: "Полная переиндексация", exact: true }).last().click();
    await page.locator("#vs-query").fill("How do I rotate a token?");
    await page.getByRole("button", { name: "Найти", exact: true }).click();
    await expect(
      page.getByText("Rotating a token replaces the stored value in place."),
    ).toBeVisible();
    await expect(page.getByRole("status")).toContainText("1 результатов");
    await expect(page.locator("article.opacity-50")).toHaveCount(1);
    expect(requests.some((body) => body.includes("How do I rotate a token?"))).toBe(true);
    await page.screenshot({ path: testInfo.outputPath("vector-stores-1440x900.png") });
    await page.getByRole("button", { name: "Изменить", exact: true }).click();
    await expect(page.locator("#vs-dimension")).toBeDisabled();
    await page.locator("#vs-name").fill("Renamed knowledge base");
    await page.getByRole("button", { name: "Сохранить", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Renamed knowledge base" })).toBeVisible();
    await rm(assertTemporaryPath(path), { recursive: true });
    await page.getByRole("button", { name: "Полная переиндексация", exact: true }).last().click();
    await expect(page.getByRole("alert")).toContainText("Таблица недоступна");
    await expect(page.getByRole("button", { name: "Сверить", exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Удалить", exact: true }).click();
    await page.getByRole("dialog").getByRole("button", { name: "Удалить", exact: true }).click();
    await expect(page.getByText("Хранилищ пока нет.", { exact: true })).toBeVisible();
    expect(errors).toEqual([]);
  } finally {
    await app.close();
    await new Promise<void>((resolve) => server.close(() => resolve()));
    directory.dispose();
  }
});
