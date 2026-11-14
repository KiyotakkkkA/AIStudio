import { createServer } from "node:http";
import { expect, test } from "@playwright/test";
import type { ConversationDto, ProviderDto, RunDto, RunHandleDto } from "@zvs/shared";
import { temporaryDirectory } from "../test/helpers/paths.ts";
import { launchStudio, rpc } from "./launch.ts";

const NOTE = [
  "---",
  "title: Остановка фоновых процессов",
  "context: Проверка отмены фоновой сборки.",
  "read_when: Перед изменением запуска процессов.",
  "update_when: При изменении механизма запуска.",
  "delete_when: После замены механизма.",
  "---",
  "",
  "Остановка затрагивает всё дерево процессов.",
  "",
].join("\n");

interface FixtureMessage {
  readonly role: string;
}

interface FixtureTool {
  readonly function: { readonly name: string };
}

test("a read note becomes a source card that opens the notes panel", async () => {
  const directory = temporaryDirectory("studio-notes-");
  const server = createServer(async (request, response) => {
    let body = "";
    for await (const chunk of request) body += String(chunk);
    if (!request.url?.endsWith("/chat/completions")) {
      response.setHeader("Content-Type", "application/json");
      response.end(JSON.stringify({ data: [{ id: "fixture-chat", context_length: 128000 }] }));
      return;
    }
    const parsed = JSON.parse(body) as {
      readonly stream?: boolean;
      readonly messages: readonly FixtureMessage[];
      readonly tools?: readonly FixtureTool[];
    };
    response.setHeader("Content-Type", "text/event-stream");
    if (parsed.stream !== true) {
      response.end("data: [DONE]\n\n");
      return;
    }
    const read = parsed.tools?.find((tool) => tool.function.name.includes("read"));
    const answered = parsed.messages.some((message) => message.role === "tool");
    const delta =
      answered || read === undefined
        ? { content: "Остановите всё дерево процессов." }
        : {
            tool_calls: [
              {
                index: 0,
                id: "call_1",
                type: "function",
                function: {
                  name: read.function.name,
                  arguments: JSON.stringify({ paths: [{ path: "chat:notes/shutdown.md" }] }),
                },
              },
            ],
          };
    response.write(`data: ${JSON.stringify({ choices: [{ delta }] })}\n\n`);
    response.end("data: [DONE]\n\n");
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("No server address");
  const app = await launchStudio(directory.path);
  try {
    const page = app.page;
    await page.setViewportSize({ width: 1440, height: 900 });
    const provider = await rpc<ProviderDto>(page, "providers.create", {
      name: "Local notes fixture",
      kind: "openai-compatible",
      adapter: "openai-compatible",
      authMode: "api",
      baseUrl: `http://127.0.0.1:${address.port}`,
      capabilities: ["text"],
      enabled: true,
      settings: { selectedModelIds: ["fixture-chat"] },
    });
    await rpc(page, "providers.probe", { id: provider.id });
    const conversation = await rpc<ConversationDto>(page, "chat.conversations.create", {
      providerId: provider.id,
      modelId: "fixture-chat",
    });
    const handle = await rpc<RunHandleDto>(page, "runs.start", {
      kind: "job",
      graph: {
        nodes: [
          {
            id: "write",
            type: "fs.apply",
            input: {
              conversationId: conversation.id,
              changes: [{ op: "write", path: "chat:notes/shutdown.md", content: NOTE }],
            },
          },
        ],
      },
    });
    await expect
      .poll(async () => (await rpc<RunDto>(page, "runs.get", { id: handle.id })).status)
      .toBe("succeeded");
    await page.locator("nav").getByText("Чат", { exact: true }).click();
    await page.getByText(conversation.title, { exact: true }).first().click();
    const input = page.getByRole("textbox", { name: /Message|Сообщение/ });
    await input.fill("Как остановить сборку?");
    await input.press("Enter");
    await expect(page.getByText("Остановите всё дерево процессов.")).toBeVisible();
    const source = page.getByRole("button", { name: /Остановка фоновых процессов.*Заметка/ });
    await expect(source).toBeVisible();
    await source.click();
    const panel = page.getByRole("complementary", { name: "Заметки" });
    await expect(panel.getByText("Остановка фоновых процессов")).toBeVisible();
    const view = page.getByRole("region", { name: "Просмотр заметки" });
    await expect(view.getByText("Остановка затрагивает всё дерево процессов.")).toBeVisible();
    await expect(view.getByText("Перед изменением запуска процессов.")).toBeVisible();
    await view.getByRole("button", { name: "Удалить" }).click();
    await page
      .getByRole("dialog", { name: "Удаление заметки" })
      .getByRole("button", { name: "Удалить" })
      .click();
    await expect(panel.getByText("Заметок нет")).toBeVisible();
    await source.click();
    await expect(page.getByText("Заметка удалена").first()).toBeVisible();
  } finally {
    await app.close();
    await new Promise<void>((resolve, reject) =>
      server.close((error) => (error ? reject(error) : resolve())),
    );
    directory.dispose();
  }
});
