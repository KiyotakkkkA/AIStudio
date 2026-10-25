import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  contract,
  type Contract,
  type McpServerDto,
  type McpServerId,
  type ToolDto,
  type ToolId,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import ToolStore from "../../src/renderer/features/tools/ToolStore.ts";
import {
  ALL_SOURCES,
  effectiveTier,
  filterTools,
  sourceCounts,
  sourceGroups,
  tierCounts,
} from "../../src/renderer/features/tools/toolFilters.ts";
import { describeSchema, schemaType } from "../../src/renderer/features/tools/schemaFields.ts";
import {
  formatLastUsed,
  unavailability,
} from "../../src/renderer/features/tools/toolPresentation.ts";

const QDRANT = "0199cc11-1111-7111-8111-000000000001" as McpServerId;
const FILES = "0199cc11-1111-7111-8111-000000000002" as McpServerId;

function id(n: number): ToolId {
  return `0199cc11-4444-7111-8111-${String(n).padStart(12, "0")}` as ToolId;
}

function server(overrides: Partial<McpServerDto> = {}): McpServerDto {
  return {
    id: QDRANT,
    name: "qdrant-local",
    namespace: "qdrant",
    transport: "stdio",
    command: "uvx",
    args: [],
    env: {},
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

function tool(n: number, key: string, overrides: Partial<ToolDto> = {}): ToolDto {
  return {
    id: id(n),
    key,
    source: "builtin",
    sourceRef: null,
    displayName: key,
    description: `${key} description`,
    inputSchema: null,
    readOnly: true,
    destructive: false,
    tier: "auto",
    enabled: true,
    available: true,
    lastUsedAt: null,
    callCount: 0,
    registeredAt: 1_000,
    updatedAt: 1_000,
    ...overrides,
  };
}

const SEARCH_SCHEMA = {
  type: "object",
  properties: {
    collection: { type: "string", description: "Collection name" },
    query: { type: "string" },
    k: { type: "integer", default: 5 },
    min_score: { type: "number", default: 0 },
    filter: {
      type: "object",
      properties: { tag: { type: "string" }, since: { type: ["string", "null"] } },
      required: ["tag"],
    },
    mode: { enum: ["dense", "hybrid"], default: "hybrid" },
    ids: { type: "array", items: { type: "string" } },
    either: { anyOf: [{ type: "string" }, { type: "integer" }] },
  },
  required: ["collection", "query"],
  additionalProperties: false,
};

const REGISTRY: ToolDto[] = [
  tool(1, "vector.search"),
  tool(2, "vector.index", { readOnly: false, tier: "ask" }),
  tool(3, "qdrant.search", {
    source: "mcp",
    sourceRef: QDRANT,
    inputSchema: SEARCH_SCHEMA,
    description: "Semantic search over a collection",
  }),
  tool(4, "qdrant.deleteCollection", {
    source: "mcp",
    sourceRef: QDRANT,
    readOnly: false,
    destructive: true,
    tier: "off",
  }),
  tool(5, "fs.readFile", { source: "mcp", sourceRef: FILES, available: false }),
  tool(6, "fs.writeFile", { source: "mcp", sourceRef: FILES, readOnly: false, tier: "ask" }),
  tool(7, "fs.move", {
    source: "mcp",
    sourceRef: FILES,
    readOnly: false,
    tier: "ask",
    enabled: false,
  }),
];

const SERVERS = [
  server(),
  server({ id: FILES, name: "filesystem", status: "failed", lastError: "spawn npx ENOENT" }),
];

test("source groups follow the registry and name MCP servers by their configured name", () => {
  const groups = sourceGroups(REGISTRY, SERVERS);
  expect(groups.map((group) => [group.key, group.label])).toEqual([
    ["builtin", "Встроенные"],
    [`mcp:${FILES}`, "filesystem"],
    [`mcp:${QDRANT}`, "qdrant-local"],
  ]);
  expect(sourceGroups(REGISTRY, [])[1]?.server).toBeNull();
});

test("filters combine source, effective tier and text, and counts are faceted", () => {
  const all = { source: ALL_SOURCES, tier: "all" as const, query: "" };
  expect(filterTools(REGISTRY, all)).toHaveLength(7);
  expect(effectiveTier(REGISTRY[6]!)).toBe("off");

  expect(tierCounts(REGISTRY, all)).toEqual({ auto: 3, ask: 2, off: 2 });
  expect(sourceCounts(REGISTRY, all)).toEqual({
    [ALL_SOURCES]: 7,
    builtin: 2,
    [`mcp:${QDRANT}`]: 2,
    [`mcp:${FILES}`]: 3,
  });

  const files = { ...all, source: `mcp:${FILES}` };
  expect(filterTools(REGISTRY, files).map((row) => row.key)).toEqual([
    "fs.move",
    "fs.readFile",
    "fs.writeFile",
  ]);
  expect(tierCounts(REGISTRY, files)).toEqual({ auto: 1, ask: 1, off: 1 });

  const off = { ...all, tier: "off" as const };
  expect(filterTools(REGISTRY, off).map((row) => row.key)).toEqual([
    "fs.move",
    "qdrant.deleteCollection",
  ]);
  expect(sourceCounts(REGISTRY, off)).toEqual({
    [ALL_SOURCES]: 2,
    [`mcp:${QDRANT}`]: 1,
    [`mcp:${FILES}`]: 1,
  });

  const query = { ...all, query: "  SEMANTIC " };
  expect(filterTools(REGISTRY, query).map((row) => row.key)).toEqual(["qdrant.search"]);
  expect(filterTools(REGISTRY, { ...query, tier: "ask" })).toEqual([]);
});

test("a JSON Schema renders as readable fields with required marks and defaults", () => {
  const view = describeSchema(SEARCH_SCHEMA);
  expect(view.kind).toBe("fields");
  expect(
    view.fields.map((field) => [
      field.path,
      field.depth,
      field.type,
      field.required,
      field.defaultValue,
    ]),
  ).toEqual([
    ["collection", 0, "string", true, null],
    ["query", 0, "string", true, null],
    ["k", 0, "integer", false, "5"],
    ["min_score", 0, "number", false, "0"],
    ["filter", 0, "object", false, null],
    ["filter.tag", 1, "string", true, null],
    ["filter.since", 1, "string | null", false, null],
    ["mode", 0, '"dense" | "hybrid"', false, '"hybrid"'],
    ["ids", 0, "string[]", false, null],
    ["either", 0, "string | integer", false, null],
  ]);
  expect(view.fields[0]?.description).toBe("Collection name");
  expect(
    schemaType({ type: "array", items: { anyOf: [{ type: "string" }, { type: "number" }] } }),
  ).toBe("(string | number)[]");

  expect(describeSchema(null).kind).toBe("none");
  expect(describeSchema({ type: "object", properties: {} }).kind).toBe("none");
  expect(describeSchema({ type: "string" })).toMatchObject({ kind: "raw" });
});

test("an offline source explains why its tools are unavailable", () => {
  const files = SERVERS[1]!;
  expect(unavailability(REGISTRY[4]!, files)).toEqual({
    title: "Источник офлайн: filesystem — ошибка подключения",
    detail: "spawn npx ENOENT",
  });
  expect(unavailability(REGISTRY[5]!, files)?.title).toBe("Сервер-источник не подключён");
  expect(unavailability(REGISTRY[4]!, null)?.title).toBe("MCP-сервер удалён");
  expect(unavailability(REGISTRY[0]!, null)).toBeNull();
});

test("presentation formats last use", () => {
  const now = 10 * 24 * 60 * 60 * 1000;
  expect(formatLastUsed(null, now)).toBe("никогда");
  expect(formatLastUsed(now - 2 * 60_000, now)).toBe("2 м назад");
  expect(formatLastUsed(now - 3 * 60 * 60_000, now)).toBe("3 ч назад");
  expect(formatLastUsed(now - 30 * 60 * 60_000, now)).toBe("вчера");
});

function fakeHost(rows: ToolDto[] = REGISTRY) {
  const bridge = createFakeBridge<Contract>();
  const registry = new Map(rows.map((row) => [row.id, { ...row }]));
  bridge.handle("tools.list", () => [...registry.values()]);
  bridge.handle("mcp.list", () => SERVERS);
  bridge.handle("tools.get", ({ id: toolId }) => ({
    ...registry.get(toolId)!,
    references: [{ kind: "scenario", refId: "s1", name: "Support triage bot" }],
  }));
  bridge.handle("tools.setTier", ({ id: toolId, tier }) => {
    const next = { ...registry.get(toolId)!, tier };
    registry.set(toolId, next);
    return next;
  });
  bridge.handle("tools.setEnabled", ({ id: toolId, enabled }) => {
    const next = { ...registry.get(toolId)!, enabled };
    registry.set(toolId, next);
    return next;
  });
  const store = new ToolStore(createIpcClient(contract, bridge), { pollMs: 0, clock: () => 5_000 });
  return { bridge, store, registry };
}

test("the store loads the registry, selects the first visible tool and its references", async () => {
  const { store } = fakeHost();
  await store.load();
  expect(store.tools).toHaveLength(7);
  expect(store.selected?.key).toBe("fs.move");
  store.setSource(`mcp:${QDRANT}`);
  expect(store.visible.map((row) => row.key)).toEqual(["qdrant.deleteCollection", "qdrant.search"]);
  await store.select(id(3));
  expect(store.detail?.references).toEqual([
    { kind: "scenario", refId: "s1", name: "Support triage bot" },
  ]);
  store.setTierFilter("auto");
  expect(store.tier).toBe("auto");
  store.setTierFilter("auto");
  expect(store.tier).toBe("all");
});

test("a host refusal to change a tier is shown, not swallowed", async () => {
  const { store, bridge } = fakeHost();
  bridge.fail("tools.setTier", AppErrorCode.PERMISSION_DENIED, "Инструмент выключен");
  await store.load();
  await store.setTier(id(1), "ask");
  expect(store.error).toBe("Инструмент выключен");
  expect(store.isBusy(id(1))).toBe(false);
});

test("raising a disabled tool above off re-enables it; off keeps it as the user left it", async () => {
  const { store, registry } = fakeHost();
  await store.load();
  await store.setTier(id(7), "auto");
  expect(registry.get(id(7))).toMatchObject({ enabled: true, tier: "auto" });
  expect(store.tierCounts.off).toBe(1);
  await store.setTier(id(7), "off");
  expect(registry.get(id(7))).toMatchObject({ enabled: true, tier: "off" });
});

test("an empty registry is an empty state, not an error", async () => {
  const { store } = fakeHost([]);
  await store.load();
  expect(store.isEmpty).toBe(true);
  expect(store.selected).toBeNull();
  expect(store.error).toBeNull();
});
