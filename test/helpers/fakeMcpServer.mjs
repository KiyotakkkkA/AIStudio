// A minimal MCP server over stdio, driven by env so one script covers every failure mode the
// driver has to survive. Started by apps/studio/tests/mcp.test.ts, never by the app.
import { writeFileSync } from "node:fs";
import process from "node:process";

const PROTOCOL = process.env.FAKE_MCP_PROTOCOL ?? "2025-06-18";
if (process.env.FAKE_MCP_PID_FILE)
  writeFileSync(process.env.FAKE_MCP_PID_FILE, String(process.pid));

const TOOLS = [
  {
    name: "search",
    title: "search",
    description: "Семантический поиск по коллекции.",
    inputSchema: { type: "object", properties: { query: { type: "string" } } },
    annotations: { readOnlyHint: true },
  },
  {
    name: "upsert",
    description: "Запись точек в коллекцию.",
    inputSchema: { type: "object" },
  },
  {
    name: "deleteCollection",
    description: "Удаление коллекции целиком.",
    inputSchema: { type: "object" },
  },
  { name: "whoami", description: "Возвращает окружение.", inputSchema: { type: "object" } },
  { name: "hang", description: "Никогда не отвечает.", inputSchema: { type: "object" } },
  { name: "crash", description: "Убивает процесс.", inputSchema: { type: "object" } },
];

function write(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function reply(id, result) {
  if (process.env.FAKE_MCP_MALFORMED === "1") {
    write({ jsonrpc: "2.0", id, result: { unexpected: "тело не по схеме" } });
    return;
  }
  write({ jsonrpc: "2.0", id, result });
}

function callTool(id, params) {
  const name = params?.name;
  if (name === "hang") return;
  if (name === "crash") {
    process.exit(7);
    return;
  }
  if (name === "whoami") {
    reply(id, {
      content: [
        {
          type: "text",
          text: JSON.stringify({
            QDRANT_API_KEY: process.env.QDRANT_API_KEY ?? null,
            QDRANT_COLLECTION: process.env.QDRANT_COLLECTION ?? null,
          }),
        },
      ],
      isError: false,
    });
    return;
  }
  if (name === "boom") {
    write({ jsonrpc: "2.0", id, error: { code: -32000, message: "инструмент отказал" } });
    return;
  }
  reply(id, {
    content: [{ type: "text", text: `${name}:${JSON.stringify(params?.arguments ?? {})}` }],
    isError: false,
  });
}

function handle(message) {
  const { id, method, params } = message;
  if (method === "initialize") {
    reply(id, {
      protocolVersion: PROTOCOL,
      capabilities: { tools: {}, resources: {}, prompts: {} },
      serverInfo: { name: "fake-qdrant", version: "0.8.1" },
      instructions: "Read qdrant://collections before searching.",
    });
    return;
  }
  if (method === "notifications/initialized" || method?.startsWith("notifications/")) return;
  if (method === "tools/list") {
    reply(id, { tools: TOOLS });
    return;
  }
  if (method === "resources/list") {
    reply(id, {
      resources: [
        { uri: "qdrant://collections", name: "collections", description: "Список коллекций." },
      ],
    });
    return;
  }
  if (method === "resources/read") {
    reply(id, {
      contents: [{ uri: params?.uri, mimeType: "text/markdown", text: "kb, docs" }],
    });
    return;
  }
  if (method === "prompts/list") {
    reply(id, { prompts: [{ name: "summarise", description: "Свести результаты поиска." }] });
    return;
  }
  if (method === "tools/call") {
    callTool(id, params);
    return;
  }
  write({ jsonrpc: "2.0", id, error: { code: -32601, message: `Unknown method: ${method}` } });
}

let buffer = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  buffer += chunk;
  let newline = buffer.indexOf("\n");
  while (newline >= 0) {
    const line = buffer.slice(0, newline).trim();
    buffer = buffer.slice(newline + 1);
    if (line) {
      try {
        handle(JSON.parse(line));
      } catch {
        process.stderr.write("fake-mcp: unreadable line\n");
      }
    }
    newline = buffer.indexOf("\n");
  }
});
process.stdin.on("end", () => process.exit(0));
