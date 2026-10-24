import assert from "node:assert/strict";
import { test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { contract, type Contract, type Json } from "@zvs/shared";
import { createEventRouter, type RoutedEvent } from "../../src/renderer/app/EventRouter.ts";
import { DEFAULT_ROUTE, ROUTES } from "../../src/renderer/app/routes.ts";
import { UiStore } from "../../src/renderer/stores/UiStore.ts";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createFakeClock } from "../../../../test/helpers/fakeClock.ts";

function settingsBridge(seed: Record<string, Json> = {}) {
  const stored = new Map<string, Json>(Object.entries(seed));
  const clock = createFakeClock();
  const bridge = createFakeBridge<Contract>();
  bridge.handle("settings.get", ({ key }) => ({ key, value: stored.get(key) }));
  bridge.handle("settings.set", ({ key, value }) => {
    stored.set(key, value);
    return { key, value, updatedAt: clock.advance(1_000) };
  });
  return { bridge, stored };
}

const client = (bridge: ReturnType<typeof createFakeBridge<Contract>>) =>
  createIpcClient(contract, bridge);

test("the store restores rail and route from the host through the real bridge contract", async () => {
  const { bridge } = settingsBridge({
    "ui.rail.collapsed": true,
    "ui.route.last": ROUTES.providers,
  });
  const ui = new UiStore(client(bridge));

  assert.equal(ui.restored, false);
  assert.equal(ui.activeRoute, DEFAULT_ROUTE);

  await ui.restore();

  assert.equal(ui.restored, true);
  assert.equal(ui.railCollapsed, true);
  assert.equal(ui.activeRoute, ROUTES.providers);
  assert.equal(ui.railWidth, 64);
  assert.deepEqual(
    bridge.calls.map((call) => call.channel),
    Array.from({ length: 9 }, () => "settings.get"),
  );
});

test("stored values that are no longer routes fall back to the default", async () => {
  const { bridge } = settingsBridge({ "ui.route.last": "/gone", "ui.rail.collapsed": "yes" });
  const ui = new UiStore(client(bridge));
  await ui.restore();

  assert.equal(ui.activeRoute, DEFAULT_ROUTE);
  assert.equal(ui.railCollapsed, false);
});

test("a failing host leaves the store usable instead of throwing", async () => {
  const bridge = createFakeBridge<Contract>();
  const ui = new UiStore(client(bridge));

  await ui.restore();

  assert.equal(ui.restored, true);
  assert.equal(ui.activeRoute, DEFAULT_ROUTE);
  ui.toggleRail();
  assert.equal(ui.railCollapsed, true);
});

test("changing the rail or the route writes through to the host once", async () => {
  const { bridge, stored } = settingsBridge();
  const ui = new UiStore(client(bridge));
  await ui.restore();

  ui.toggleRail();
  ui.setRailCollapsed(true);
  ui.setActiveRoute(ROUTES.tools);
  ui.setActiveRoute(ROUTES.tools);
  ui.setActiveRoute("/not-a-route");
  await Promise.resolve();

  assert.equal(bridge.calls.filter((call) => call.channel === "settings.set").length, 2);
  assert.equal(stored.get("ui.rail.collapsed"), true);
  assert.equal(stored.get("ui.route.last"), ROUTES.tools);
});

test("the theme mode restores, rejects unknown values and writes through once", async () => {
  const { bridge, stored } = settingsBridge({ "ui.theme": "dark" });
  const ui = new UiStore(client(bridge));
  await ui.restore();
  assert.equal(ui.theme, "dark");

  ui.setTheme("light");
  ui.setTheme("light");
  await Promise.resolve();
  assert.equal(stored.get("ui.theme"), "light");
  assert.equal(bridge.calls.filter((call) => call.channel === "settings.set").length, 1);

  const fallback = new UiStore(client(settingsBridge({ "ui.theme": "sepia" }).bridge));
  await fallback.restore();
  assert.equal(fallback.theme, "system");
});

test("effects follow the OS until a preset is picked, then the preset decides", async () => {
  const { bridge, stored } = settingsBridge();
  const ui = new UiStore(client(bridge));
  await ui.restore();

  assert.equal(ui.fxShown, "full");
  assert.equal(ui.motion, "full");
  assert.equal(ui.effects, "full");

  ui.setOsReducedMotion(true);
  assert.equal(ui.fxShown, "reduced");
  assert.equal(ui.motion, "reduced");
  assert.equal(ui.effects, "full");

  ui.setFxPreset("none");
  await Promise.resolve();
  assert.equal(ui.motion, "none");
  assert.equal(ui.effects, "none");
  assert.equal(stored.get("ui.fx.preset"), "none");

  ui.setOsReducedMotion(false);
  assert.equal(ui.motion, "none");
});

test("custom starts from the current levels once and keeps its own levels afterwards", async () => {
  const { bridge, stored } = settingsBridge({ "ui.fx.preset": "reduced" });
  const ui = new UiStore(client(bridge));
  await ui.restore();
  assert.equal(ui.motion, "reduced");

  ui.setFxPreset("custom");
  assert.equal(ui.customMotion, "reduced");
  assert.equal(ui.customEffects, "reduced");

  ui.setCustomMotion("none");
  ui.setCustomEffects("full");
  await Promise.resolve();
  assert.equal(ui.motion, "none");
  assert.equal(ui.effects, "full");
  assert.equal(stored.get("ui.fx.motion"), "none");
  assert.equal(stored.get("ui.fx.effects"), "full");

  ui.setFxPreset("full");
  assert.equal(ui.motion, "full");
  ui.setFxPreset("custom");
  assert.equal(ui.motion, "none");
  assert.equal(ui.effects, "full");
});

test("stored effect values that are not levels fall back to the defaults", async () => {
  const { bridge } = settingsBridge({
    "ui.fx.preset": "loud",
    "ui.fx.motion": 3,
    "ui.fx.effects": "custom",
  });
  const ui = new UiStore(client(bridge));
  await ui.restore();
  assert.equal(ui.fxPreset, null);
  assert.equal(ui.customMotion, "full");
  assert.equal(ui.customEffects, "full");
});

test("events pushed through the bridge reach the router that subscribed to the stream", () => {
  const bridge = createFakeBridge<Contract>();
  const router = createEventRouter({ logger: { log: () => {} } });
  router.connect((handler) => bridge.subscribe(handler));

  const streamId = "0199aa11-1111-7111-8111-000000000001";
  const seen: RoutedEvent[] = [];
  router.subscribe(streamId as RoutedEvent["streamId"], (event) => seen.push(event));

  bridge.emit({ type: "progress", streamId, seq: 0, ts: 1, done: 1, total: 2 });
  bridge.emit({ type: "end", streamId, seq: 1, ts: 2, outcome: { status: "ok" } });
  bridge.emit("nonsense");

  assert.deepEqual(
    seen.map((event) => event.type),
    ["progress", "end"],
  );
  assert.equal(bridge.subscribers, 1);
  router.dispose();
  assert.equal(bridge.subscribers, 0);
});
