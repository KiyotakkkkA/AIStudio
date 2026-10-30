import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { AppErrorCode, contract, type Contract, type Json } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createFakeClock } from "../../../../test/helpers/fakeClock.ts";
import { UiStore } from "../../src/renderer/stores/UiStore.ts";
import AppSettingsStore from "../../src/renderer/features/preferences/AppSettingsStore.ts";
import { PREFERENCE_SECTIONS } from "../../src/renderer/stores/UiStore.ts";
import { VOICE_INPUT } from "../../src/renderer/features/voice/voiceCopy.ts";
import {
  AUTOSTART_FAILED,
  GENERAL_ROWS,
  NOTIFICATION_KIND_ROWS,
  NOTIFICATIONS_ROW,
  PREFERENCE_GROUPS,
  SECTION_LABELS,
  SETTING_SAVE_FAILED,
  cliRows,
  fontSizeLabel,
  fontStack,
  interfaceZoom,
} from "../../src/renderer/features/preferences/preferencesPresentation.ts";
import { CLI_CHANNEL, CLI_VERSION } from "../../src/renderer/app/appEnv.ts";

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

test("the about section describes the CLI from the app environment", () => {
  const rows = cliRows();
  expect(rows.map((row) => row.label)).toEqual(["Версия CLI", "Канал CLI"]);
  expect(rows[0]?.value).toBe(CLI_VERSION);
  expect(rows[1]?.value).toBe(CLI_CHANNEL);
});

function appSettings(seed: Record<string, Json> = {}) {
  const stored = new Map<string, Json>(Object.entries(seed));
  const clock = createFakeClock();
  const bridge = createFakeBridge<Contract>()
    .handle("settings.get", ({ key }) => ({ key, value: stored.get(key) }))
    .handle("settings.set", ({ key, value }) => {
      stored.set(key, value);
      return { key, value, updatedAt: clock.advance(1_000) };
    });
  return { bridge, stored, store: new AppSettingsStore(createIpcClient(contract, bridge)) };
}

test("the application group lists its sections in order and the extensions group holds components and the CLI", () => {
  expect(
    PREFERENCE_GROUPS.map((group) => [
      group.label,
      group.sections.map((section) => SECTION_LABELS[section]),
    ]),
  ).toEqual([
    [
      "Приложение",
      ["Общие", "Профиль", "Внешний вид", "Уведомления", "Память", "Данные", "О системе"],
    ],
    ["Расширения", ["Компоненты", VOICE_INPUT, "CLI"]],
  ]);
  expect(PREFERENCE_GROUPS.flatMap((group) => group.sections).sort()).toEqual(
    [...PREFERENCE_SECTIONS].sort(),
  );
  expect(GENERAL_ROWS.map((row) => row.key)).toEqual(["app.autostart", "app.tray"]);
  expect(NOTIFICATIONS_ROW.key).toBe("notifications.enabled");
  expect(NOTIFICATION_KIND_ROWS.map((row) => row.key)).toEqual([
    "notifications.approval",
    "notifications.turn",
    "notifications.download",
  ]);
});

test("application settings load with their defaults and persist each toggle", async () => {
  const { stored, store } = appSettings({ "app.tray": true, "notifications.turn": "yes" });
  await store.load();
  expect(store.values).toEqual({
    "app.autostart": false,
    "app.tray": true,
    "notifications.enabled": true,
    "notifications.approval": true,
    "notifications.turn": true,
    "notifications.download": true,
    "memory.enabled": true,
    "memory.model": true,
  });
  await store.set("notifications.enabled", false);
  await store.set("app.autostart", true);
  expect(stored.get("notifications.enabled")).toBe(false);
  expect(stored.get("app.autostart")).toBe(true);
});

test("notification kinds keep their values while notifications are off", async () => {
  const { stored, store } = appSettings({ "notifications.download": false });
  await store.load();
  await store.set("notifications.enabled", false);
  await store.set("notifications.enabled", true);
  expect(store.values["notifications.download"]).toBe(false);
  expect(stored.get("notifications.download")).toBe(false);
});

test("a refused change rolls the toggle back and says why", async () => {
  const { bridge, store } = appSettings();
  await store.load();
  bridge.fail("settings.set", AppErrorCode.NATIVE_ERROR, "host text");
  await store.set("app.autostart", true);
  expect(store.values["app.autostart"]).toBe(false);
  expect(store.errors["app.autostart"]).toBe(AUTOSTART_FAILED);
  await store.set("app.tray", true);
  expect(store.values["app.tray"]).toBe(false);
  expect(store.errors["app.tray"]).toBe(SETTING_SAVE_FAILED);
  expect(store.pending).toEqual({});
});
