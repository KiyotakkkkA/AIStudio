import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  CatalogueItemDto,
  contract,
  ToolchainDto,
  type Contract,
  type ToolchainRuntime,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import DownloadStore from "../../src/renderer/features/downloads/DownloadStore.ts";
import ToolchainStore from "../../src/renderer/features/toolchain/ToolchainStore.ts";
import {
  availableActions,
  DISCONNECT_LABEL,
  ACTION_LABELS,
  removalLabel,
  toolchainErrorCopy,
  toolchainRuntimeOf,
  TOOLCHAIN_ERROR_COPY,
  UNKNOWN_TOOLCHAIN_ERROR,
  updateLine,
} from "../../src/renderer/features/toolchain/toolchainPresentation.ts";

const STREAM = "0199cc11-1111-7111-8111-000000000003";

function dto(runtime: ToolchainRuntime, patch: Partial<ToolchainDto> = {}): ToolchainDto {
  return ToolchainDto.parse({
    runtime,
    displayName: runtime === "node" ? "Node.js" : "uv",
    status: "missing",
    minimumVersion: runtime === "node" ? "18.0.0" : "0.4.0",
    updateAvailable: false,
    ...patch,
  });
}

function setup(initial: ToolchainDto[]) {
  let rows = initial;
  const bridge = createFakeBridge<Contract>().handle("toolchain.list", () => rows);
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new ToolchainStore(createIpcClient(contract, bridge), events);
  return {
    store,
    bridge,
    events,
    setRows(next: ToolchainDto[]) {
      rows = next;
    },
  };
}

test("mount lists quietly and checks for updates only when a managed copy is connected", async () => {
  const quiet = setup([dto("node"), dto("uv")]);
  await quiet.store.mount();
  expect(quiet.bridge.calls.map((call) => call.payload)).toEqual([{ checkUpdates: false }]);
  expect(quiet.store.ordered.map((row) => row.runtime)).toEqual(["node", "uv"]);
  quiet.store.dispose();

  const managed = setup([
    dto("node", {
      status: "ready",
      source: "managed",
      version: "22.11.0",
      executablePath: "C:/data/node/node.exe",
    }),
    dto("uv"),
  ]);
  await managed.store.mount();
  expect(managed.bridge.calls.map((call) => call.payload)).toEqual([
    { checkUpdates: false },
    { checkUpdates: true },
  ]);
  managed.store.dispose();
});

test("an action replaces its runtime and a failure is shown in host or mapped wording", async () => {
  const { store, bridge } = setup([dto("node"), dto("uv")]);
  await store.load();
  bridge.handle("toolchain.install", ({ runtime }) =>
    dto(runtime, { status: "installing", refs: ["toolchain:node:22.21.0:node.zip"] }),
  );
  expect(await store.install("node")).toBe(true);
  expect(store.ordered[0]?.status).toBe("installing");
  expect(store.installing).toBe(true);
  expect(store.busy.node).toBeUndefined();

  bridge.fail("toolchain.scan", AppErrorCode.NOT_FOUND, "uv не найден в PATH");
  expect(await store.scan("uv")).toBe(false);
  expect(store.errors.uv).toBe("uv не найден в PATH");

  bridge.fail("toolchain.update", AppErrorCode.PROVIDER_UNREACHABLE, "error sending request");
  await store.update("uv");
  expect(store.errors.uv).toBe(TOOLCHAIN_ERROR_COPY[AppErrorCode.PROVIDER_UNREACHABLE]);
  expect(toolchainErrorCopy(new Error("boom"))).toBe(UNKNOWN_TOOLCHAIN_ERROR);
});

test("a download ending while a runtime installs refreshes the list without a scan", async () => {
  vi.useFakeTimers();
  try {
    const harness = setup([dto("node", { status: "installing" }), dto("uv")]);
    await harness.store.mount();
    const before = harness.bridge.calls.length;
    harness.setRows([
      dto("node", {
        status: "ready",
        source: "managed",
        version: "22.21.0",
        executablePath: "C:/data/node/node.exe",
      }),
      dto("uv"),
    ]);
    harness.events.dispatch({
      streamId: STREAM,
      seq: 1,
      ts: 1,
      type: "end",
      outcome: { status: "ok" },
    });
    await vi.advanceTimersByTimeAsync(300);
    expect(harness.bridge.calls.length).toBe(before + 1);
    expect(harness.store.ordered[0]?.status).toBe("ready");
    harness.store.dispose();
  } finally {
    vi.useRealTimers();
  }
});

test("the card offers the actions that fit the runtime's state", () => {
  expect(availableActions(dto("node"))).toEqual(["install", "scan", "select"]);
  expect(availableActions(dto("node", { status: "installing" }))).toEqual([]);
  const ready = dto("node", {
    status: "ready",
    source: "managed",
    version: "22.11.0",
    latestVersion: "22.21.0",
    updateAvailable: true,
    executablePath: "C:/data/node/node.exe",
  });
  expect(availableActions(ready)).toEqual(["update", "scan", "select", "remove"]);
  expect(updateLine(ready)).toContain("22.21.0");
  expect(removalLabel(ready)).toBe(ACTION_LABELS.remove);
  expect(removalLabel({ source: "path" })).toBe(DISCONNECT_LABEL);
  expect(toolchainRuntimeOf("toolchain:uv")).toBe("uv");
  expect(toolchainRuntimeOf("toolchain:uv:0.9.5:uv.zip")).toBeUndefined();
  expect(toolchainRuntimeOf("runtime:llama-cpp-cpu")).toBeUndefined();
});

test("a toolchain row on the downloads page installs, updates and removes through toolchain channels", async () => {
  const item = (state: "available" | "update" | "installed") =>
    CatalogueItemDto.parse({
      ref: "toolchain:node",
      kind: "runtime",
      source: "github",
      name: "node",
      displayName: "Node.js",
      sizeBytes: 1,
      url: "https://nodejs.org/en/download",
      state,
      downloadable: state !== "installed",
    });
  const bridge = createFakeBridge<Contract>()
    .handle("downloads.list", () => [])
    .handle("downloads.catalogue", () => [])
    .handle("settings.get", ({ key }) => ({ key }))
    .handle("toolchain.install", ({ runtime }) => dto(runtime, { status: "installing" }))
    .handle("toolchain.update", ({ runtime }) => dto(runtime, { status: "installing" }))
    .handle("toolchain.remove", ({ runtime }) => dto(runtime));
  const store = new DownloadStore(
    createIpcClient(contract, bridge),
    createEventRouter({ logger: { log: vi.fn() } }),
  );
  await store.install(item("available"));
  await store.install(item("update"));
  expect(store.isRemovable(item("installed"))).toBe(true);
  expect(store.isRemovable(item("available"))).toBe(false);
  await store.uninstall(item("installed"));
  const toolchainCalls = bridge.calls.filter((call) => call.channel.startsWith("toolchain."));
  expect(toolchainCalls).toEqual([
    { channel: "toolchain.install", payload: { runtime: "node" } },
    { channel: "toolchain.update", payload: { runtime: "node" } },
    { channel: "toolchain.remove", payload: { runtime: "node" } },
  ]);
  expect(bridge.calls.some((call) => call.channel === "downloads.start")).toBe(false);

  bridge.fail("toolchain.install", AppErrorCode.CONFLICT, "Эта среда уже загружается");
  await store.install(item("available"));
  expect(store.runtimeError).toBe("Эта среда уже загружается");
});
