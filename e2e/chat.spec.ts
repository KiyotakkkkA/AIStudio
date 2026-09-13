import { createServer } from "node:http";
import { createRequire } from "node:module";
import { join } from "node:path";
import { _electron as electron, expect, test, type Page } from "@playwright/test";
import { STUDIO_ROOT, temporaryDirectory, assertTemporaryPath } from "../test/helpers/paths.ts";
import { RustCore } from "../apps/studio/src/host/drivers/rust/RustCore.ts";
import { nativeTargetTriple } from "../apps/studio/src/host/platform/paths.ts";

async function rpc<T>(page: Page, channel: string, payload?: unknown): Promise<T> {
  const result = (await page.evaluate(({ channel, payload }) => window.zvs.call(channel, payload), {
    channel,
    payload,
  })) as { ok: boolean; data: T };
  expect(result).toMatchObject({ ok: true });
  return result.data;
}

test("chat streams grounded answers, preserves scroll, cancels and fits a narrow thread", async ({ browserName }, testInfo) => {
  expect(browserName).toBe("chromium");
  const directory = temporaryDirectory("studio-chat-ui-");
  let turn = 0;
  const server = createServer(async (request, response) => {
    let body = "";
    for await (const chunk of request) body += String(chunk);
    if (request.url?.endsWith("/chat/completions")) {
      response.setHeader("Content-Type", "text/event-stream");
      if (!JSON.parse(body).stream) {
        response.end("data: [DONE]\n\n");
        return;
      }
      const current = ++turn;
      const chunks =
        current === 1
          ? [
              "Rotation is **safe**.\n\n",
              "```html\n<img src=x onerror=alert(1)>\n```\n\n",
              ...Array.from(
                { length: 60 },
                (_, index) =>
                  `Paragraph ${index + 1}: Running scenarios keep their current connection; the next request uses the rotated key.\n\n`,
              ),
            ]
          : Array.from({ length: 150 }, () => "Partial response continues. ");
      let index = 0;
      const timer = setInterval(() => {
        if (index >= chunks.length) {
          clearInterval(timer);
          response.end("data: [DONE]\n\n");
          return;
        }
        response.write(
          `data: ${JSON.stringify({ choices: [{ delta: { content: chunks[index++] } }] })}\n\n`,
        );
      }, 40);
      response.on("close", () => clearInterval(timer));
    } else {
      response.setHeader("Content-Type", "application/json");
      response.end(
        JSON.stringify(
          request.method === "GET"
            ? { data: [{ id: "fixture-chat", context_length: 128000 }] }
            : { data: [{ embedding: [1, 0, 0], index: 0 }], embeddings: [[1, 0, 0]] },
        ),
      );
    }
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
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await app.evaluate(({ BrowserWindow }) => {
      const window = BrowserWindow.getAllWindows()[0]!;
      window.unmaximize();
      window.setContentSize(1440, 900);
    });
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.locator("nav").getByText("Чат", { exact: true }).click();
    await expect(
      page.getByRole("link", { name: /Open AI Providers|Открыть провайдеров ИИ/ }),
    ).toBeVisible();
    const provider = await rpc<{ id: string }>(page, "providers.create", {
      name: "Local chat fixture",
      kind: "ollama",
      adapter: "openai-compatible",
      authMode: "api",
      baseUrl: `http://127.0.0.1:${address.port}`,
      capabilities: ["text", "embedding"],
      enabled: true,
    });
    await rpc(page, "providers.probe", { id: provider.id });
    const store = await rpc<{ id: string }>(page, "vectorStores.create", {
      name: "Product knowledge base",
      embeddingProviderId: provider.id,
      embeddingModelId: "fixture-chat",
      dimension: 3,
    });
    const core = RustCore.fromPaths({
      nativeAddonPath: join(STUDIO_ROOT, "resources/native", nativeTargetTriple(), "zvs-core.node"),
    });
    await core.upsertVectors(assertTemporaryPath(join(directory.path, "vectors", store.id)), [
      {
        id: "rotation",
        vector: [1, 0, 0],
        documentId: "manual",
        chunkIndex: 0,
        path: "secrets/rotation.md#L42",
        payload: {
          text: "Running scenarios keep their connection. The next request uses the new key.",
          chunkCount: 1,
        },
      },
    ]);
    await page.locator("nav").getByText("Секреты", { exact: true }).click();
    await page.locator("nav").getByText("Чат", { exact: true }).click();
    await page.getByRole("checkbox", { name: /Product knowledge base/ }).check();
    const input = page.getByRole("textbox", { name: /Message|Сообщение/ });
    await input.fill("Is rotation safe?");
    await input.press("Enter");
    const allow = page.getByRole("button", { name: /Allow once|Разрешить один раз/ });
    await allow.click();
    await allow.click();
    await expect(page.getByText("Paragraph 8:", { exact: false })).toBeVisible();
    const viewport = page
      .locator('[aria-label="Лента чата"], [aria-label="Chat thread"]')
      .locator("div.overflow-y-auto")
      .first();
    await viewport.evaluate((element) => {
      element.scrollTop = 0;
      element.dispatchEvent(new Event("scroll"));
    });
    await expect(
      page.getByRole("button", { name: /Jump to latest|К последнему|Вниз|последним/i }),
    ).toBeVisible();
    await expect(input).toBeEnabled();
    expect(await viewport.evaluate((element) => element.scrollTop)).toBe(0);
    await expect(page.locator("summary").first()).toContainText("Product knowledge base");
    await expect(page.getByText("secrets/rotation.md#L42", { exact: false }).last()).toBeVisible();
    expect(await page.locator("article img, article script").count()).toBe(0);
    await page.screenshot({ path: testInfo.outputPath("chat-1440.png") });
    for (const width of [1100, 966]) {
      await page.setViewportSize({ width, height: 900 });
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
      );
      const thread = page.locator('[aria-label="Лента чата"], [aria-label="Chat thread"]');
      expect(await thread.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(
        true,
      );
      await page.screenshot({ path: testInfo.outputPath(`chat-${width}.png`) });
    }
    await input.fill("Continue");
    await input.press("Enter");
    await allow.click();
    await allow.click();
    await expect(page.getByText("Partial response continues.", { exact: false })).toBeVisible();
    await page.getByRole("button", { name: /^(Stop|Остановить|Стоп)$/ }).click();
    await expect(input).toBeEnabled();
    const conversations = await rpc<{ id: string }[]>(page, "chat.conversations.list");
    const detail = await rpc<{ messages: { content: string; partial: boolean }[] }>(
      page,
      "chat.conversations.get",
      { id: conversations[0]!.id },
    );
    expect(detail.messages.at(-1)).toMatchObject({ partial: true });
    expect(detail.messages.at(-1)?.content).toContain("Partial response");
    expect(errors).toEqual([]);
  } finally {
    await app.close();
    server.closeAllConnections();
    await new Promise<void>((resolve) => server.close(() => resolve()));
    directory.dispose();
  }
});
