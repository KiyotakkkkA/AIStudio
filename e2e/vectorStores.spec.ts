import { createServer } from "node:http";
import { join } from "node:path";
import { rm } from "node:fs/promises";
import { expect, test } from "@playwright/test";
import { STUDIO_ROOT, temporaryDirectory, assertTemporaryPath } from "../test/helpers/paths.ts";
import { RustCore } from "../apps/studio/src/host/drivers/rust/RustCore.ts";
import { nativeTargetTriple } from "../apps/studio/src/host/platform/paths.ts";
import { launchStudio } from "./launch.ts";

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
      JSON.stringify(
        request.method === "GET"
          ? { data: [{ id: "fixture-embedding" }] }
          : {
              embeddings: [[1, 0, 0]],
              embedding: [1, 0, 0],
              data: [{ embedding: [1, 0, 0], index: 0 }],
            },
      ),
    );
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("No server address");
  const app = await launchStudio(directory.path);
  try {
    const page = app.page;
    await page.setViewportSize({ width: 1440, height: 900 });
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    const provider = await page.evaluate(
      async (port) =>
        window.zvs.call("providers.create", {
          name: "UI embedding fixture",
          kind: "openai-compatible",
          adapter: "openai-compatible",
          authMode: "api",
          baseUrl: `http://127.0.0.1:${port}`,
          capabilities: ["embedding"],
          enabled: true,
          settings: { selectedModelIds: ["fixture-embedding"] },
        }),
      address.port,
    );
    expect(provider).toMatchObject({ ok: true });
    const providerId = (provider as { data: { id: string } }).data.id;
    const probe = await page.evaluate(
      (id) => window.zvs.call("providers.probe", { id }),
      providerId,
    );
    expect(probe).toMatchObject({ ok: true, data: { outcome: { kind: "ok" } } });
    await page.locator("nav").getByText("Векторные хранилища", { exact: true }).click();
    await expect(page.getByText("Хранилищ пока нет.", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Новое хранилище", exact: true }).click();
    await page.locator("#vs-name").fill("Product knowledge base");
    await page
      .locator("#vs-description")
      .fill("Manuals, changelogs and FAQ for the ZVS toolchain.");
    await page.getByText("Выберите провайдер", { exact: true }).click();
    await page.getByText("UI embedding fixture", { exact: true }).click();
    await page.getByText("Выберите модель", { exact: true }).click();
    await page.getByText("fixture-embedding", { exact: true }).click();
    await page.locator("#vs-dimension").fill("3");
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
        text: "Rotating a token replaces the stored value in place.",
        section: "",
        page: null,
        payload: { chunkCount: 2 },
      },
      {
        id: "b",
        vector: [-1, 0, 0],
        documentId: "manual",
        chunkIndex: 1,
        path: "faq/general.md#L96",
        text: "Secrets are stored per workspace.",
        section: "",
        page: null,
        payload: { chunkCount: 2 },
      },
    ]);
    await page.getByRole("button", { name: "Сверить", exact: true }).click();
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
    await page.getByTitle("Сверить статистику с таблицей", { exact: true }).click();
    await expect(page.getByRole("alert")).toContainText("Таблица недоступна");
    await expect(page.getByRole("button", { name: "Сверить", exact: true })).toHaveCount(2);
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
