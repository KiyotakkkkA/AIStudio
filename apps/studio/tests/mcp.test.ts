import { join } from "node:path";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { AppErrorCode, contract, StartRunInput, type Json } from "@zvs/shared";
import { REPO_ROOT } from "../../../test/helpers/paths.ts";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { createSecretService } from "../../../test/helpers/secretService.ts";
import { createEventBus, type EventBus } from "../src/host/platform/events.ts";
import { NodeRegistry } from "../src/host/kernel/NodeRegistry.ts";
import { RunService } from "../src/host/services/RunService.ts";
import { ToolRegistry } from "../src/host/tools/ToolRegistry.ts";
import { deriveNamespace, McpService, toolKey } from "../src/host/services/McpService.ts";
import type { SecretService } from "../src/host/services/SecretService.ts";
import type { LogLevel } from "../src/host/platform/logger.ts";
import { createMcpHandlers } from "../src/host/ipc/mcp.ts";
import { createToolHandlers } from "../src/host/ipc/tools.ts";

const SERVER = join(REPO_ROOT, "test", "helpers", "fakeMcpServer.mjs");

interface LogEntry {
  level: LogLevel;
  message: string;
  fields: Record<string, unknown>;
}

let db: TemporaryDatabase;
let secrets: SecretService;
let tools: ToolRegistry;
let nodes: NodeRegistry;
let mcp: McpService;
let logs: LogEntry[];

const logger = {
  log: (level: LogLevel, _scope: string, message: string, fields: Record<string, unknown> = {}) => {
    logs.push({ level, message, fields });
  },
  close: () => undefined,
};

function service(overrides: Partial<ConstructorParameters<typeof McpService>[0]> = {}): McpService {
  return new McpService({
    data: db.client,
    tools,
    nodes,
    secrets,
    logger,
    requestTimeoutMs: 5_000,
    callTimeoutMs: 5_000,
    backoffMs: 10,
    maxBackoffMs: 20,
    ...overrides,
  });
}

function stdioServer(
  name: string,
  env: Record<
    string,
    { kind: "literal"; value: string } | { kind: "secret"; secretId: string }
  > = {},
  args: string[] = [],
) {
  return mcp.create({
    name,
    transport: "stdio",
    command: process.execPath,
    args: [SERVER, ...args],
    env,
  });
}

beforeEach(() => {
  db = temporaryDatabase();
  logs = [];
  secrets = createSecretService(db.client);
  tools = new ToolRegistry({ data: db.client, logger });
  nodes = new NodeRegistry();
  mcp = service();
});

afterEach(async () => {
  await mcp.dispose();
  db.dispose();
});

test("a name becomes a namespace and a tool name becomes a key", () => {
  expect(deriveNamespace("qdrant-local")).toBe("qdrantLocal");
  expect(deriveNamespace("GitHub MCP")).toBe("githubMcp");
  expect(deriveNamespace("2024 server")).toBe("server");
  expect(toolKey("qdrant", "deleteCollection")).toBe("qdrant.deleteCollection");
  expect(toolKey("qdrant", "list collections!")).toBe("qdrant.list-collections");
});

test("a second server with a colliding name gets its own namespace", () => {
  expect(stdioServer("qdrant").namespace).toBe("qdrant");
  expect(stdioServer("Qdrant!").namespace).toBe("qdrant2");
});

test("a stdio server hands back a handshake, its tools, resources and prompts", async () => {
  const server = stdioServer("qdrant");
  const result = await mcp.test(server.id);
  expect(result).toMatchObject({
    ok: true,
    protocolVersion: "2025-06-18",
    serverName: "fake-qdrant",
    serverVersion: "0.8.1",
    toolCount: 6,
    resourceCount: 1,
    promptCount: 1,
    error: null,
  });
  const detail = mcp.get(server.id);
  expect(detail.status).toBe("connected");
  expect(detail.protocolVersion).toBe("2025-06-18");
  expect(detail.capabilities.filter((row) => row.kind === "prompt").map((row) => row.name)).toEqual(
    ["summarise"],
  );
  expect(detail.capabilities.find((row) => row.kind === "resource")?.uri).toBe(
    "qdrant://collections",
  );
});

test("discovery registers tools with their schemas and a safe default tier", async () => {
  const server = stdioServer("qdrant");
  await mcp.start(server.id);
  const registered = new Map(tools.list({ source: "mcp" }).map((row) => [row.key, row]));
  expect([...registered.keys()].sort()).toEqual([
    "qdrant.crash",
    "qdrant.deleteCollection",
    "qdrant.hang",
    "qdrant.search",
    "qdrant.upsert",
    "qdrant.whoami",
  ]);
  // A read-only hint earns `auto`, a plain tool `ask`, and a name that removes something `off`.
  expect(registered.get("qdrant.search")).toMatchObject({ tier: "auto", readOnly: true });
  expect(registered.get("qdrant.upsert")).toMatchObject({ tier: "ask", destructive: false });
  expect(registered.get("qdrant.deleteCollection")).toMatchObject({
    tier: "off",
    destructive: true,
  });
  expect(registered.get("qdrant.search")?.inputSchema).toMatchObject({ type: "object" });
  expect(registered.get("qdrant.search")?.sourceRef).toBe(server.id);
  expect(nodes.list().map((node) => node.type)).toContain("qdrant.search");
});

test("a discovered tool is callable and its result is normalised", async () => {
  const server = stdioServer("qdrant");
  await mcp.start(server.id);
  const result = await mcp.call({ serverId: server.id, tool: "search" }, { query: "вопрос" });
  expect(result.isError).toBe(false);
  expect(result.text).toBe('search:{"query":"вопрос"}');
  expect(await mcp.callByToolKey("qdrant.search", { query: "ещё" })).toMatchObject({
    text: 'search:{"query":"ещё"}',
  });
});

test("secret env values are resolved at launch and never persisted or logged", async () => {
  const secret = secrets.create({
    type: "custom",
    name: "Qdrant — local cluster",
    scope: "personal",
    value: "sk-qdrant-0123456789abcdef",
  });
  const server = stdioServer("qdrant", {
    QDRANT_API_KEY: { kind: "secret", secretId: secret.id },
    QDRANT_COLLECTION: { kind: "literal", value: "product-knowledge-base" },
  });
  await mcp.start(server.id);
  const result = await mcp.call({ serverId: server.id, tool: "whoami" }, {});
  // The child really received the plaintext…
  expect(JSON.parse(result.text)).toEqual({
    QDRANT_API_KEY: "sk-qdrant-0123456789abcdef",
    QDRANT_COLLECTION: "product-knowledge-base",
  });
  // …and nothing else did.
  const row = db.client.repositories.mcpServers.findById(server.id);
  expect(row?.env).toEqual({
    QDRANT_API_KEY: { kind: "secret", secretId: secret.id },
    QDRANT_COLLECTION: { kind: "literal", value: "product-knowledge-base" },
  });
  expect(JSON.stringify(mcp.get(server.id))).not.toContain("sk-qdrant");
  expect(JSON.stringify(logs)).not.toContain("sk-qdrant");
  expect(JSON.stringify(mcp.health(server.id))).not.toContain("sk-qdrant");
  expect(secrets.usage(secret.id)).toEqual([{ kind: "mcp_server", id: server.id }]);
});

test("a server that never starts fails with an error naming the command", async () => {
  const server = mcp.create({
    name: "missing",
    transport: "stdio",
    command: "definitely-not-a-real-binary-zvs",
    args: ["--serve"],
  });
  await expect(mcp.start(server.id)).rejects.toMatchObject({
    code: AppErrorCode.MCP_UNAVAILABLE,
  });
  const row = mcp.get(server.id);
  expect(row.status).toBe("failed");
  expect(row.lastError).toContain("definitely-not-a-real-binary-zvs");
  expect(tools.list({ source: "mcp", includeUnavailable: false })).toEqual([]);
});

test("a hung call times out without taking the connection with it", async () => {
  const server = stdioServer("qdrant");
  await mcp.start(server.id);
  await expect(
    mcp.call({ serverId: server.id, tool: "hang", timeoutMs: 120 }, {}),
  ).rejects.toMatchObject({ code: AppErrorCode.MCP_TIMEOUT });
  // The server is still there, so the next call goes through.
  expect((await mcp.call({ serverId: server.id, tool: "search" }, {})).isError).toBe(false);
  const health = mcp.health(server.id);
  expect(health.errors).toBe(1);
  expect(health.recentLog.some((line) => line.line.includes("hang"))).toBe(true);
});

test("a malformed response is normalised to UNKNOWN and the payload stays host-side", async () => {
  const server = mcp.create({
    name: "broken",
    transport: "stdio",
    command: process.execPath,
    args: [SERVER],
    env: { FAKE_MCP_MALFORMED: { kind: "literal", value: "1" } },
  });
  await expect(mcp.start(server.id)).rejects.toMatchObject({ code: AppErrorCode.UNKNOWN });
  expect(mcp.get(server.id).status).toBe("failed");
  const reported = logs.find((entry) => entry.message.includes("malformed"));
  expect(reported?.fields.raw).toContain("тело не по схеме");
  // The rail carries the fact, never the payload.
  expect(
    mcp.health(server.id).recentLog.some((line) => line.line.includes("тело не по схеме")),
  ).toBe(false);
});

test("a server that speaks an unsupported protocol version is refused", async () => {
  const server = mcp.create({
    name: "ancient",
    transport: "stdio",
    command: process.execPath,
    args: [SERVER],
    env: { FAKE_MCP_PROTOCOL: { kind: "literal", value: "1999-01-01" } },
  });
  await expect(mcp.start(server.id)).rejects.toMatchObject({
    code: AppErrorCode.MCP_UNAVAILABLE,
  });
  expect(mcp.get(server.id).lastError).toContain("1999-01-01");
});

test("a server that dies marks its tools unavailable, fails in flight, and restarts", async () => {
  const server = stdioServer("qdrant");
  await mcp.start(server.id);
  expect(tools.list({ source: "mcp", includeUnavailable: false })).toHaveLength(6);
  await expect(mcp.call({ serverId: server.id, tool: "crash" }, {})).rejects.toMatchObject({
    code: AppErrorCode.MCP_UNAVAILABLE,
  });
  await vi.waitFor(() => {
    expect(mcp.get(server.id).status).toBe("failed");
    expect(tools.list({ source: "mcp", includeUnavailable: false })).toEqual([]);
  });
  // Backoff brings it back on its own.
  await vi.waitFor(
    () => {
      expect(mcp.get(server.id).status).toBe("connected");
      expect(tools.list({ source: "mcp", includeUnavailable: false })).toHaveLength(6);
    },
    { timeout: 10_000 },
  );
  expect(mcp.health(server.id).restarts).toBeGreaterThanOrEqual(1);
  expect((await mcp.call({ serverId: server.id, tool: "search" }, {})).isError).toBe(false);
});

test("stopping is not a death and never schedules a restart", async () => {
  const server = stdioServer("qdrant");
  await mcp.start(server.id);
  await mcp.stop(server.id);
  expect(mcp.get(server.id).status).toBe("stopped");
  await new Promise((done) => setTimeout(done, 120));
  expect(mcp.get(server.id).status).toBe("stopped");
  expect(mcp.health(server.id).restarts).toBe(0);
});

test("the health rail reports uptime, calls, errors and a median latency", async () => {
  const server = stdioServer("qdrant");
  await mcp.start(server.id);
  await mcp.call({ serverId: server.id, tool: "search" }, {});
  await mcp.call({ serverId: server.id, tool: "search" }, {});
  const health = mcp.health(server.id);
  expect(health.status).toBe("connected");
  expect(health.uptimeMs).toBeGreaterThanOrEqual(0);
  expect(health.calls).toBeGreaterThanOrEqual(3);
  expect(health.errors).toBe(0);
  expect(health.medianLatencyMs).not.toBeNull();
  expect(health.recentLog.at(-1)?.line).toContain("initialize protocol 2025-06-18");
});

test("removing a server withdraws its tools and its nodes", async () => {
  const server = stdioServer("qdrant");
  await mcp.start(server.id);
  await mcp.remove(server.id);
  expect(tools.list({ source: "mcp", includeUnavailable: false })).toEqual([]);
  expect(nodes.list().map((node) => node.type)).not.toContain("qdrant.search");
  expect(() => mcp.get(server.id)).toThrow();
});

test("disabling a server takes its tools out of reach", async () => {
  const server = stdioServer("qdrant");
  await mcp.start(server.id);
  await mcp.update({ id: server.id, enabled: false });
  expect(mcp.get(server.id).status).toBe("disabled");
  expect(tools.list({ source: "mcp", includeUnavailable: false })).toEqual([]);
  await expect(mcp.start(server.id)).rejects.toMatchObject({ code: AppErrorCode.CONFLICT });
});

test("an http server handshakes and discovers over the wire", async () => {
  const calls: { body: unknown; headers: Record<string, string> }[] = [];
  const fetchStub = vi.fn(async (_url: string, init: RequestInit) => {
    const body = JSON.parse(String(init.body)) as { id?: string; method: string };
    calls.push({ body, headers: init.headers as Record<string, string> });
    if (body.id === undefined) return new Response(null, { status: 202 });
    const result =
      body.method === "initialize"
        ? {
            protocolVersion: "2025-06-18",
            capabilities: { tools: {} },
            serverInfo: { name: "github", version: "2.0.0" },
          }
        : body.method === "tools/list"
          ? { tools: [{ name: "listIssues", description: "Задачи", inputSchema: {} }] }
          : {};
    return new Response(JSON.stringify({ jsonrpc: "2.0", id: body.id, result }), {
      status: 200,
      headers: { "content-type": "application/json", "mcp-session-id": "session-1" },
    });
  });
  const secret = secrets.create({
    type: "custom",
    name: "GitHub token",
    scope: "personal",
    value: "ghp_0123456789abcdefghijklmnopqrstuvwx",
  });
  mcp = service({ fetch: fetchStub });
  const server = mcp.create({
    name: "github",
    transport: "http",
    url: "https://mcp.example.com/mcp",
    headers: { Authorization: { kind: "secret", secretId: secret.id } },
  });
  await mcp.start(server.id);
  expect(mcp.get(server.id)).toMatchObject({ status: "connected", serverName: "github" });
  expect(tools.list({ source: "mcp" }).map((row) => row.key)).toEqual(["github.listIssues"]);
  // The static token rides along, the session id is honoured, and nothing is logged.
  expect(calls[0]?.headers.Authorization).toBe("ghp_0123456789abcdefghijklmnopqrstuvwx");
  expect(calls.at(-1)?.headers["mcp-session-id"]).toBe("session-1");
  expect(JSON.stringify(logs)).not.toContain("ghp_0123456789");
});

test("an http server that rejects the token fails with a clear error", async () => {
  mcp = service({ fetch: vi.fn(async () => new Response("no", { status: 401 })) });
  const server = mcp.create({
    name: "github",
    transport: "http",
    url: "https://mcp.example.com/mcp",
  });
  await expect(mcp.start(server.id)).rejects.toMatchObject({ code: AppErrorCode.MCP_UNAVAILABLE });
  expect(mcp.get(server.id).lastError).toContain("401");
});

test("a run calls an MCP tool only within the registered ceiling", async () => {
  const server = stdioServer("qdrant");
  await mcp.start(server.id);
  const bus: EventBus = createEventBus({ senders: () => [] });
  const runs = new RunService({
    data: db.client,
    events: bus,
    registry: nodes,
    tools,
    approvalTimeoutMs: 200,
  });
  try {
    const allowed = runs.start(
      StartRunInput.parse({
        kind: "scenario",
        subjectId: "s1",
        graph: { nodes: [{ id: "a", type: "qdrant.search", input: { query: "вопрос" } }] },
      }),
    );
    await runs.wait(allowed.id);
    expect(runs.get(allowed.id).status).toBe("succeeded");
    // `deleteCollection` was registered `off`, so admission refuses it before the server sees it.
    const denied = runs.start(
      StartRunInput.parse({
        kind: "scenario",
        subjectId: "s1",
        graph: { nodes: [{ id: "a", type: "qdrant.deleteCollection", input: {} }] },
      }),
    );
    await runs.wait(denied.id);
    expect(runs.get(denied.id).outcome?.code).toBe(AppErrorCode.PERMISSION_DENIED);
    expect(tools.usage(tools.list().find((row) => row.key === "qdrant.search")!.id)).toMatchObject({
      callCount: 1,
    });
  } finally {
    await runs.dispose();
    bus.dispose();
  }
});

test("a call made while the server is down brings it back rather than failing", async () => {
  const server = stdioServer("qdrant");
  await mcp.start(server.id);
  await mcp.stop(server.id);
  const result = await mcp.call({ serverId: server.id, tool: "search" }, { query: "x" } as Record<
    string,
    Json
  >);
  expect(result.isError).toBe(false);
  expect(mcp.get(server.id).status).toBe("connected");
});

test("the page's channels round-trip through the contract", async () => {
  const handlers = createMcpHandlers(mcp);
  const created = contract["mcp.create"].output.parse(
    handlers["mcp.create"](
      contract["mcp.create"].input.parse({
        name: "qdrant",
        transport: "stdio",
        command: process.execPath,
        args: [SERVER],
      }),
    ),
  );
  expect(contract["mcp.list"].output.parse(handlers["mcp.list"]())).toHaveLength(1);
  const result = contract["mcp.test"].output.parse(await handlers["mcp.test"]({ id: created.id }));
  expect(result).toMatchObject({ ok: true, toolCount: 6 });
  const detail = contract["mcp.get"].output.parse(handlers["mcp.get"]({ id: created.id }));
  expect(detail.capabilities.filter((row) => row.kind === "tool")).toHaveLength(6);
  const health = contract["mcp.health"].output.parse(handlers["mcp.health"]({ id: created.id }));
  expect(health).toMatchObject({ status: "connected", restarting: false });
  const stopped = contract["mcp.stop"].output.parse(await handlers["mcp.stop"]({ id: created.id }));
  expect(stopped.status).toBe("stopped");
  const started = contract["mcp.start"].output.parse(
    await handlers["mcp.start"]({ id: created.id }),
  );
  expect(started.status).toBe("connected");
  const updated = contract["mcp.update"].output.parse(
    await handlers["mcp.update"]({ id: created.id, enabled: false }),
  );
  expect(updated.status).toBe("disabled");
  const refused = contract["mcp.start"].output.parse(
    await handlers["mcp.start"]({ id: created.id }),
  );
  expect(refused.status).toBe("disabled");
  expect(
    contract["mcp.remove"].output.parse(await handlers["mcp.remove"]({ id: created.id })),
  ).toEqual({ id: created.id, removed: true });
  expect(handlers["mcp.list"]()).toEqual([]);
});

test("a tier set through the tools channel is what the registry holds for an MCP tool", async () => {
  const server = stdioServer("qdrant");
  await mcp.test(server.id);
  const key = mcp.get(server.id).capabilities.find((row) => row.name === "search")?.toolKey;
  const tool = tools
    .list({ source: "mcp", includeUnavailable: true })
    .find((row) => row.key === key);
  expect(tool?.sourceRef).toBe(server.id);
  const handlers = createToolHandlers(tools);
  handlers["tools.setTier"]({ id: tool!.id, tier: "off" });
  handlers["tools.setEnabled"]({ id: tool!.id, enabled: false });
  await mcp.test(server.id);
  expect(tools.get(tool!.id)).toMatchObject({ tier: "off", enabled: false, available: true });
});

test("health says a crashed server is waiting to restart", async () => {
  await mcp.dispose();
  mcp = service({ backoffMs: 5_000, maxBackoffMs: 5_000 });
  const server = stdioServer("qdrant");
  await mcp.start(server.id);
  await expect(mcp.call({ serverId: server.id, tool: "crash" }, {})).rejects.toBeDefined();
  await vi.waitFor(() => {
    expect(mcp.health(server.id)).toMatchObject({ status: "failed", restarting: true });
  });
  await mcp.stop(server.id);
  expect(mcp.health(server.id).restarting).toBe(false);
});

test("a command that cannot start fails once and is not restarted", async () => {
  const server = mcp.create({
    name: "ghost",
    transport: "stdio",
    command: "zvs-definitely-not-a-command",
  });
  const result = await mcp.test(server.id);
  expect(result.ok).toBe(false);
  expect(result.error).toContain("ENOENT");
  await new Promise((done) => setTimeout(done, 120));
  expect(mcp.get(server.id).status).toBe("failed");
  expect(mcp.health(server.id)).toMatchObject({ restarts: 0, restarting: false });
});

test("stdio servers get the isolated toolchain env, and a server's own env wins", async () => {
  await mcp.dispose();
  const seen: NodeJS.ProcessEnv[] = [];
  mcp = service({
    toolchainDir: join("C:", "data", "mcp-toolchain"),
    spawn: (_command, _args, options) => {
      seen.push(options.env);
      throw Object.assign(new Error("spawn ENOENT"), { code: "ENOENT" });
    },
  });
  const server = mcp.create({
    name: "npx",
    transport: "stdio",
    command: "npx",
    env: { UV_CACHE_DIR: { kind: "literal", value: "D:/mine" } },
  });
  await mcp.test(server.id);
  expect(seen[0]?.npm_config_cache).toBe(join("C:", "data", "mcp-toolchain", "npm-cache"));
  expect(seen[0]?.UV_CACHE_DIR).toBe("D:/mine");
});
