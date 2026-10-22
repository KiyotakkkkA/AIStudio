import assert from "node:assert/strict";
import { test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  contract,
  type Contract,
  type McpCapabilityDto,
  type McpHealthDto,
  type McpServerDetailDto,
  type McpServerDto,
  type McpServerId,
  type SecretId,
  type ToolDto,
  type ToolId,
  type ToolTier,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import ConnectionStore from "../../src/renderer/features/connections/ConnectionStore.ts";
import { ServerFormVm } from "../../src/renderer/features/connections/ServerFormVm.ts";
import { joinArgs, splitArgs } from "../../src/renderer/features/connections/args.ts";
import {
  failureCopy,
  handshakeLine,
  serverState,
} from "../../src/renderer/features/connections/connectionPresentation.ts";
import { SERVER_REGISTRY } from "../../src/renderer/features/connections/serverRegistry.ts";

const SERVER = "0199cc11-1111-7111-8111-000000000001" as McpServerId;
const OTHER = "0199cc11-1111-7111-8111-000000000002" as McpServerId;
const SECRET = "0199cc11-3333-7111-8111-000000000001" as SecretId;
const SEARCH = "0199cc11-4444-7111-8111-000000000001" as ToolId;
const DROP = "0199cc11-4444-7111-8111-000000000002" as ToolId;
const FOREIGN = "0199cc11-4444-7111-8111-000000000003" as ToolId;

function server(overrides: Partial<McpServerDto> = {}): McpServerDto {
  return {
    id: SERVER,
    name: "qdrant-local",
    namespace: "qdrantLocal",
    transport: "stdio",
    command: "uvx",
    args: ["mcp-server-qdrant", "--url", "http://127.0.0.1:6333"],
    env: {
      QDRANT_API_KEY: { kind: "secret", secretId: SECRET },
      QDRANT_COLLECTION: { kind: "literal", value: "product-knowledge-base" },
    },
    url: null,
    headers: {},
    enabled: true,
    startup: "launch",
    status: "connected",
    oauth: "none",
    protocolVersion: "2025-06-18",
    serverName: "qdrant",
    serverVersion: "0.8.1",
    instructions: null,
    lastError: null,
    lastConnectedAt: 1_000,
    toolCount: 2,
    resourceCount: 0,
    promptCount: 0,
    createdAt: 1_000,
    updatedAt: 1_000,
    ...overrides,
  };
}

function capability(name: string, toolKey: string, destructive = false): McpCapabilityDto {
  return {
    id: crypto.randomUUID(),
    serverId: SERVER,
    kind: "tool",
    name,
    title: null,
    description: `${name} tool`,
    toolKey,
    uri: null,
    mimeType: null,
    schema: null,
    readOnly: !destructive,
    destructive,
    discoveredAt: 1_000,
  };
}

function tool(id: ToolId, key: string, sourceRef: string, tier: ToolTier): ToolDto {
  return {
    id,
    key,
    source: "mcp",
    sourceRef,
    displayName: key,
    description: "",
    inputSchema: null,
    readOnly: tier === "auto",
    destructive: tier === "off",
    tier,
    enabled: tier !== "off",
    available: true,
    lastUsedAt: null,
    callCount: 0,
    registeredAt: 1_000,
    updatedAt: 1_000,
  };
}

function health(overrides: Partial<McpHealthDto> = {}): McpHealthDto {
  return {
    serverId: SERVER,
    status: "connected",
    uptimeMs: 60_000,
    restarts: 0,
    restarting: false,
    calls: 3,
    errors: 0,
    medianLatencyMs: 64,
    recentLog: [{ at: 1_000, level: "info", line: "initialize protocol 2025-06-18" }],
    ...overrides,
  };
}

function fakeHost() {
  const bridge = createFakeBridge<Contract>();
  const servers = new Map<string, McpServerDto>([[SERVER, server()]]);
  const registry = new Map<string, ToolDto>([
    [SEARCH, tool(SEARCH, "qdrantLocal.search", SERVER, "auto")],
    [DROP, tool(DROP, "qdrantLocal.deleteCollection", SERVER, "off")],
    [FOREIGN, tool(FOREIGN, "github.search", OTHER, "ask")],
  ]);
  const capabilities = [
    capability("search", "qdrantLocal.search"),
    capability("deleteCollection", "qdrantLocal.deleteCollection", true),
  ];
  let tested = 0;
  const detail = (id: string): McpServerDetailDto => ({ ...servers.get(id)!, capabilities });

  bridge.handle("mcp.list", () => [...servers.values()]);
  bridge.handle("mcp.get", ({ id }) => detail(id));
  bridge.handle("mcp.health", ({ id }) => health({ serverId: id as McpServerId }));
  bridge.handle("secrets.list", () => []);
  bridge.handle("mcp.create", (input) => {
    const created = server({ ...input, id: OTHER, status: "stopped", lastConnectedAt: null });
    servers.set(OTHER, created);
    return created;
  });
  bridge.handle("mcp.update", (input) => {
    const next = { ...servers.get(input.id)!, ...input, updatedAt: 2_000 };
    servers.set(input.id, next);
    return next;
  });
  bridge.handle("mcp.test", () => {
    tested += 1;
    return {
      ok: true,
      latencyMs: 128,
      protocolVersion: "2025-06-18",
      serverName: null,
      serverVersion: "0.8.1",
      toolCount: 2,
      resourceCount: 0,
      promptCount: 0,
      error: null,
    };
  });
  bridge.handle("tools.list", (filter) =>
    [...registry.values()].filter(
      (row) => filter.source === undefined || row.source === filter.source,
    ),
  );
  bridge.handle("tools.usage", ({ id }) => ({
    id,
    key: registry.get(id)!.key,
    callCount: 0,
    lastUsedAt: null,
    references:
      id === SEARCH
        ? [
            { kind: "scenario" as const, refId: "s1", name: "Support triage" },
            { kind: "skill" as const, refId: "k1", name: "Research" },
          ]
        : [{ kind: "scenario" as const, refId: "s1", name: "Support triage" }],
  }));
  bridge.handle("tools.setTier", ({ id, tier }) => {
    const next = { ...registry.get(id)!, tier };
    registry.set(id, next);
    return next;
  });
  bridge.handle("tools.setEnabled", ({ id, enabled }) => {
    const next = { ...registry.get(id)!, enabled };
    registry.set(id, next);
    return next;
  });

  const store = new ConnectionStore(createIpcClient(contract, bridge), { pollMs: 0 });
  return {
    bridge,
    store,
    servers,
    registry,
    tested: () => tested,
  };
}

test("arguments split on whitespace, honour quotes and round-trip through join", () => {
  assert.deepEqual(splitArgs(`mcp-server-qdrant --url  http://127.0.0.1:6333`), [
    "mcp-server-qdrant",
    "--url",
    "http://127.0.0.1:6333",
  ]);
  assert.deepEqual(splitArgs(`--root "C:\\My Files" 'a b' ""`), [
    "--root",
    "C:\\My Files",
    "a b",
    "",
  ]);
  const args = ["-y", "C:\\Program Files\\x", 'say "hi"', "", "plain"];
  assert.deepEqual(splitArgs(joinArgs(args)), args);
  assert.equal(joinArgs(["C:\\data", "x"]), "C:\\data x");
});

test("the form shows command, args and env for stdio and url and headers for http", () => {
  const vm = new ServerFormVm();
  assert.deepEqual(vm.fields, ["command", "args", "env"]);
  assert.equal(vm.shows("url"), false);
  vm.setTransport("http");
  assert.deepEqual(vm.fields, ["url", "headers"]);
  assert.equal(vm.shows("env"), false);
  assert.equal(vm.shows("headers"), true);
});

test("switching transport keeps the other side's values but sends only the visible ones", () => {
  const vm = new ServerFormVm({ server: server() });
  assert.equal(vm.argsText, "mcp-server-qdrant --url http://127.0.0.1:6333");
  assert.equal(vm.dirty, false);

  vm.setTransport("http");
  vm.setUrl("https://mcp.example.com/");
  vm.addRow("headers", "secret");
  const header = vm.headers[0]!;
  vm.setRowName("headers", header.id, "Authorization");
  vm.setRowSecret("headers", header.id, SECRET);
  assert.equal(vm.dirty, true);
  assert.equal(vm.validate(), true);
  const http = vm.toUpdateInput();
  assert.equal(http.transport, "http");
  assert.equal(http.url, "https://mcp.example.com/");
  assert.equal(http.command, null);
  assert.deepEqual(http.args, []);
  assert.deepEqual(http.env, {});
  assert.deepEqual(http.headers, { Authorization: { kind: "secret", secretId: SECRET } });

  vm.setTransport("stdio");
  assert.equal(vm.command, "uvx");
  assert.equal(vm.env.length, 2);
  const stdio = vm.toUpdateInput();
  assert.equal(stdio.url, null);
  assert.deepEqual(stdio.headers, {});
  assert.deepEqual(stdio.args, ["mcp-server-qdrant", "--url", "http://127.0.0.1:6333"]);
  assert.deepEqual(stdio.env, {
    QDRANT_API_KEY: { kind: "secret", secretId: SECRET },
    QDRANT_COLLECTION: { kind: "literal", value: "product-knowledge-base" },
  });
  assert.equal(contract["mcp.update"].input.safeParse(stdio).success, true);
  assert.equal(contract["mcp.update"].input.safeParse(http).success, true);
});

test("validation follows the transport and refuses a secret row without a secret", () => {
  const vm = new ServerFormVm();
  assert.equal(vm.validate(), false);
  assert.ok(vm.errorOf("name"));
  assert.ok(vm.errorOf("command"));
  assert.equal(vm.errorOf("url"), undefined);

  vm.setName("local");
  vm.setCommand("npx");
  vm.addRow("env", "secret");
  vm.addRow("env", "literal");
  const [secretRow, literalRow] = vm.env;
  vm.setRowName("env", secretRow!.id, "API_KEY");
  vm.setRowName("env", literalRow!.id, "API_KEY");
  assert.equal(vm.validate(), false);
  assert.ok(vm.errorOf(`env.${secretRow!.id}.value`));
  assert.ok(vm.errorOf(`env.${literalRow!.id}.name`));

  vm.setRowSecret("env", secretRow!.id, SECRET);
  vm.setRowName("env", literalRow!.id, "1BAD");
  assert.equal(vm.validate(), false);
  vm.setRowName("env", literalRow!.id, "MODE");
  vm.setArgsText(`"unterminated`);
  assert.equal(vm.validate(), false);
  assert.ok(vm.errorOf("args"));
  vm.setArgsText("-y pkg");
  assert.equal(vm.validate(), true);

  vm.setTransport("http");
  assert.equal(vm.validate(), false);
  assert.ok(vm.errorOf("url"));
  assert.equal(vm.errorOf("command"), undefined);
  vm.setUrl("ftp://nope");
  assert.equal(vm.validate(), false);
  vm.setUrl("http://127.0.0.1:8080/mcp");
  assert.equal(vm.validate(), true);
  assert.equal(contract["mcp.create"].input.safeParse(vm.toCreateInput()).success, true);
});

test("a registry preset fills the form and leaves secret rows for the picker", () => {
  const preset = SERVER_REGISTRY.find((entry) => entry.id === "qdrant")!;
  const vm = new ServerFormVm({ preset });
  assert.equal(vm.isNew, true);
  assert.equal(vm.command, "uvx");
  const secretRow = vm.env.find((row) => row.kind === "secret")!;
  assert.equal(secretRow.secretId, null);
  assert.equal(vm.validate(), false);
  vm.setRowSecret("env", secretRow.id, SECRET);
  vm.setRowLiteral("env", vm.env.find((row) => row.name === "COLLECTION_NAME")!.id, "docs");
  assert.equal(vm.validate(), true);
});

test("tier and enable changes write through the registry and the page reads them back", async () => {
  const { store, registry, bridge } = fakeHost();
  await store.load();
  assert.equal(store.selectedId, SERVER);
  assert.deepEqual(
    store.toolRows.map((row) => [row.capability.name, row.tool?.tier]),
    [
      ["search", "auto"],
      ["deleteCollection", "off"],
    ],
  );
  assert.equal(
    store.tools.some((entry) => entry.id === FOREIGN),
    false,
  );
  assert.equal(store.enabledToolCount, 1);

  await store.setToolTier(SEARCH, "ask");
  assert.equal(registry.get(SEARCH)?.tier, "ask");
  assert.equal(store.toolRows[0]?.tool?.tier, "ask");
  assert.deepEqual(
    bridge.calls.filter((call) => call.channel === "tools.setTier").map((call) => call.payload),
    [{ id: SEARCH, tier: "ask" }],
  );

  await store.setToolEnabled(DROP, true);
  assert.equal(registry.get(DROP)?.enabled, true);
  assert.equal(store.enabledToolCount, 2);

  registry.set(SEARCH, { ...registry.get(SEARCH)!, tier: "off", enabled: false });
  await store.select(SERVER);
  assert.equal(store.toolRows[0]?.tool?.tier, "off");
  assert.equal(store.enabledToolCount, 1);
});

test("used-by counts distinct scenarios and skills across the server's tools", async () => {
  const { store } = fakeHost();
  await store.load();
  assert.deepEqual(store.usedBy, { scenarios: 1, skills: 1 });
  assert.equal(store.selectedHealth?.medianLatencyMs, 64);
  assert.equal(store.selectedState, "connected");
});

test("testing an unsaved form saves it first, then runs the real handshake", async () => {
  const { store, bridge, tested } = fakeHost();
  await store.load();
  store.requestCreate(SERVER_REGISTRY.find((entry) => entry.id === "time")!);
  assert.equal(store.form?.isNew, true);
  await store.test();
  assert.equal(tested(), 1);
  assert.equal(store.selectedId, OTHER);
  assert.equal(store.form?.isNew, false);
  assert.equal(store.testResult?.ok, true);
  const channels = bridge.calls.map((call) => call.channel);
  assert.ok(channels.indexOf("mcp.create") < channels.indexOf("mcp.test"));
});

test("an invalid form is not saved and not tested", async () => {
  const { store, tested, bridge } = fakeHost();
  await store.load();
  store.requestCreate(null);
  await store.test();
  assert.equal(tested(), 0);
  assert.equal(
    bridge.calls.some((call) => call.channel === "mcp.create"),
    false,
  );
  assert.ok(store.form?.errorOf("name"));
});

test("leaving a dirty form asks first", async () => {
  const { store } = fakeHost();
  await store.load();
  store.form?.setName("renamed");
  store.requestCreate(null);
  assert.equal(store.pending?.kind, "create");
  assert.equal(store.form?.isNew, false);
  store.confirmPending();
  assert.equal(store.form?.isNew, true);
});

test("the enable switch persists immediately for a saved server", async () => {
  const { store, servers } = fakeHost();
  await store.load();
  await store.setServerEnabled(false);
  assert.equal(servers.get(SERVER)?.enabled, false);
  assert.equal(store.form?.enabled, false);
  assert.equal(store.form?.dirty, false);
});

test("a server that asks for an OAuth sign-in reads as re-auth with a browser hint", () => {
  const message = "Сервер требует входа: нажмите «Войти», чтобы авторизоваться в браузере";
  assert.equal(
    serverState(
      server({ transport: "http", status: "failed", oauth: "required", lastError: message }),
      null,
    ),
    "reauth",
  );
  assert.match(failureCopy(message).hint ?? "", /браузер/);
});

test("states cover never connected, connected, re-auth, disabled and crashed-and-restarting", () => {
  assert.equal(serverState(server({ status: "stopped", lastConnectedAt: null }), null), "never");
  assert.equal(serverState(server(), null), "connected");
  assert.equal(
    serverState(
      server({
        transport: "http",
        status: "failed",
        lastError: "MCP-сервер отклонил учётные данные (401)",
      }),
      null,
    ),
    "reauth",
  );
  assert.equal(serverState(server({ enabled: false, status: "disabled" }), null), "disabled");
  assert.equal(
    serverState(
      server({ status: "failed", lastError: "process exited" }),
      health({ status: "failed", restarting: true }),
    ),
    "restarting",
  );
  assert.equal(
    serverState(server({ status: "failed", lastError: "process exited" }), health()),
    "failed",
  );
  assert.equal(
    handshakeLine({
      ok: true,
      latencyMs: 128,
      protocolVersion: "2025-06-18",
      serverName: null,
      serverVersion: "0.8.1",
      toolCount: 6,
      resourceCount: 0,
      promptCount: 0,
      error: null,
    }),
    "handshake ok · 128 ms · protocol 2025-06-18 · server v0.8.1",
  );
});

test("the server search matches name, namespace and command, ignoring case", () => {
  const { store } = fakeHost();
  store.servers = [
    server(),
    server({ id: OTHER, name: "github", namespace: "github", command: "npx", url: null }),
  ];
  assert.equal(store.visibleServers.length, 2);
  store.setQuery("  QDRANT ");
  assert.deepEqual(
    store.visibleServers.map((entry) => entry.id),
    [SERVER],
  );
  store.setQuery("npx");
  assert.deepEqual(
    store.visibleServers.map((entry) => entry.id),
    [OTHER],
  );
  store.setQuery("nothing");
  assert.deepEqual(store.visibleServers, []);
});
