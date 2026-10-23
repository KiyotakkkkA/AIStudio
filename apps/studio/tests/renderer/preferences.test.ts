import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { AppErrorCode, contract, DeviceProfileDto, type Contract, type Json } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createFakeClock } from "../../../../test/helpers/fakeClock.ts";
import { UiStore } from "../../src/renderer/stores/UiStore.ts";
import AboutStore from "../../src/renderer/features/preferences/AboutStore.ts";
import {
  DEVICE_UNAVAILABLE,
  deviceRows,
  fontSizeLabel,
  fontStack,
  interfaceZoom,
} from "../../src/renderer/features/preferences/preferencesPresentation.ts";
import { formatBytes } from "../../src/renderer/features/downloads/downloadPresentation.ts";

function settings(seed: Record<string, Json> = {}) {
  const stored = new Map<string, Json>(Object.entries(seed));
  const clock = createFakeClock();
  const bridge = createFakeBridge<Contract>()
    .handle("settings.get", ({ key }) => ({ key, value: stored.get(key) }))
    .handle("settings.set", ({ key, value }) => {
      stored.set(key, value);
      return { key, value, updatedAt: clock.advance(1_000) };
    });
  return { stored, ui: new UiStore(createIpcClient(contract, bridge)) };
}

const device = DeviceProfileDto.parse({
  platform: "win32",
  arch: "x64",
  cpuModel: "AMD Ryzen 7 7840HS",
  cpuCores: 16,
  totalMemoryBytes: 32 * 1024 ** 3,
  freeMemoryBytes: 12 * 1024 ** 3,
  freeDiskBytes: 0,
  gpu: { vendor: "NVIDIA", model: "GeForce RTX 4060", vramBytes: 8 * 1024 ** 3, discrete: true },
  tier: "high",
  measuredAt: 1,
});

test("appearance preferences restore from the host and persist when changed", async () => {
  const { stored, ui } = settings({
    "ui.rail.themepicker": false,
    "ui.font.family": "mono",
    "ui.font.size": 15,
  });
  await ui.restore();
  expect([ui.railThemePicker, ui.fontFamily, ui.fontSize]).toEqual([false, "mono", 15]);
  ui.setRailThemePicker(true);
  ui.setFontFamily("system");
  ui.setFontSize(12);
  await Promise.resolve();
  await Promise.resolve();
  expect(stored.get("ui.rail.themepicker")).toBe(true);
  expect(stored.get("ui.font.family")).toBe("system");
  expect(stored.get("ui.font.size")).toBe(12);
});

test("unknown stored fonts fall back to the defaults", async () => {
  const { ui } = settings({ "ui.font.family": "comic", "ui.font.size": 40 });
  await ui.restore();
  expect([ui.fontFamily, ui.fontSize]).toEqual(["onest", 13]);
});

test("the preferences dialog opens on a section and closes", () => {
  const { ui } = settings();
  expect(ui.preferences).toBeNull();
  ui.openPreferences();
  expect(ui.preferences).toBe("appearance");
  ui.openPreferences("about");
  expect(ui.preferences).toBe("about");
  ui.closePreferences();
  expect(ui.preferences).toBeNull();
});

test("fonts and sizes map to a font stack and an interface zoom", () => {
  expect(fontStack("onest")).toBeNull();
  expect(fontStack("mono")).toBe("var(--font-mono)");
  expect(interfaceZoom(13)).toBe(1);
  expect(interfaceZoom(15)).toBe(15 / 13);
  expect(fontSizeLabel(13)).toBe("13 px · обычный");
  expect(fontSizeLabel(16)).toBe("16 px");
});

test("the about section describes the computer and loads it once", async () => {
  const rows = deviceRows(device);
  expect(rows.map((row) => row.label)).toEqual([
    "Операционная система",
    "Процессор",
    "Оперативная память",
    "Видеокарта",
    "Свободно на диске",
  ]);
  expect(rows[0]?.value).toBe("Windows · x64");
  expect(rows[3]?.value).toBe(`GeForce RTX 4060 · ${formatBytes(8 * 1024 ** 3)}`);
  expect(rows[4]?.value).toBe("Не измерено");

  const bridge = createFakeBridge<Contract>().handle("system.device", () => device);
  const about = new AboutStore(createIpcClient(contract, bridge));
  await about.load();
  await about.load();
  expect(about.device?.cpuCores).toBe(16);
  expect(bridge.calls).toHaveLength(1);

  const failing = new AboutStore(
    createIpcClient(
      contract,
      createFakeBridge<Contract>().fail("system.device", AppErrorCode.NATIVE_ERROR, "x"),
    ),
  );
  await failing.load();
  expect(failing.error).toBe(DEVICE_UNAVAILABLE);
});
