import { mkdir, readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import {
  ImageAttachmentDto,
  type ConversationDto,
  type ImageDataDto,
  type ProviderDto,
  type RunDto,
  type RunHandleDto,
  type StepDto,
} from "@zvs/shared";
import { assertTemporaryPath, temporaryDirectory } from "../test/helpers/paths.ts";
import { launchStudio, rpc } from "./launch.ts";

test("file tools access their own conversation directory and reject another owner", async () => {
  const directory = temporaryDirectory("studio-conversation-files-");
  const server = createServer((_request, response) => {
    response.setHeader("Content-Type", "application/json");
    response.end(JSON.stringify({ data: [{ id: "fixture-chat", context_length: 128000 }] }));
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("No server address");
  const app = await launchStudio(directory.path);
  try {
    const page = app.page;
    const provider = await rpc<ProviderDto>(page, "providers.create", {
      name: "Local conversation files fixture",
      kind: "openai-compatible",
      adapter: "openai-compatible",
      authMode: "api",
      baseUrl: `http://127.0.0.1:${address.port}`,
      capabilities: ["text"],
      enabled: true,
      settings: { selectedModelIds: ["fixture-chat"] },
    });
    await rpc(page, "providers.probe", { id: provider.id });
    const createConversation = () =>
      rpc<ConversationDto>(page, "chat.conversations.create", {
        providerId: provider.id,
        modelId: "fixture-chat",
      });
    const conversation = await createConversation();
    const owner = assertTemporaryPath(join(directory.path, "conversations", conversation.id));
    const folder = join(owner, "attachments", "browser");
    const file = join(folder, "note.txt");
    const run = async (type: string, input: Record<string, unknown>, status = "succeeded") => {
      const handle = await rpc<RunHandleDto>(page, "runs.start", {
        kind: "job",
        graph: {
          nodes: [{ id: "tool", type, input: { ...input, conversationId: conversation.id } }],
        },
      });
      await expect
        .poll(async () => (await rpc<RunDto>(page, "runs.get", { id: handle.id })).status)
        .toBe(status);
      return rpc<StepDto[]>(page, "runs.steps", { id: handle.id });
    };
    await run("fs.apply", { changes: [{ op: "write", path: file, content: "needle" }] });
    await run("fs.apply", {
      changes: [{ op: "edit", path: file, edits: [{ old: "needle", new: "changed" }] }],
    });
    expect(await readFile(file, "utf8")).toBe("changed");
    await run("fs.list", { path: owner, recursive: true });
    await run("fs.search", { path: folder, query: "changed" });
    await run("fs.read", { paths: [{ path: file }] });
    const screenshot = assertTemporaryPath(join(folder, "screen-fixture-0.jpg"));
    await mkdir(folder, { recursive: true });
    await writeFile(screenshot, Buffer.from([255, 216, 255, 217]));
    const paths = [screenshot];
    if (process.platform === "win32") {
      paths.push(
        `\\\\?\\${screenshot}`,
        screenshot.toUpperCase(),
        screenshot.replaceAll("\\", "/"),
      );
    }
    for (const path of paths) {
      const [step] = await run("fs.read", {
        paths: [{ path, viewImage: true }],
        imageViewing: true,
      });
      expect(step?.output).toMatchObject({ imageOwner: conversation.id });
      const output = step?.output;
      if (!output || typeof output !== "object" || Array.isArray(output)) {
        throw new Error("Missing image output");
      }
      const images = ImageAttachmentDto.array().length(1).parse(output.images);
      const image = await rpc<ImageDataDto>(page, "chat.images.read", {
        id: conversation.id,
        image: images[0],
      });
      expect(image.dataUrl).toBe("data:image/jpeg;base64,/9j/2Q==");
    }
    const other = await createConversation();
    const secret = assertTemporaryPath(
      join(directory.path, "conversations", other.id, "private.txt"),
    );
    await mkdir(join(directory.path, "conversations", other.id), { recursive: true });
    await writeFile(secret, "private");
    await run("fs.read", { paths: [{ path: secret }] }, "failed");
    await run(
      "fs.apply",
      { changes: [{ op: "write", path: secret, content: "changed" }] },
      "failed",
    );
    expect(await readFile(secret, "utf8")).toBe("private");
  } finally {
    await app.close();
    await new Promise<void>((resolve, reject) =>
      server.close((error) => (error ? reject(error) : resolve())),
    );
    directory.dispose();
  }
});
