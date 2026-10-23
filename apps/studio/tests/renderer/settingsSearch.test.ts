import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { contract, type Contract } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { UiStore } from "../../src/renderer/stores/UiStore.ts";
import {
  FONT_FAMILY_LABEL,
  FONT_SIZE_LABEL,
  NOTIFICATION_KIND_ROWS,
  NOTIFICATIONS_ROW,
} from "../../src/renderer/features/preferences/preferencesPresentation.ts";
import {
  SETTING_ENTRIES,
  SETTING_IDS,
  searchSettings,
} from "../../src/renderer/features/preferences/settingsIndex.ts";

test("a query matches setting titles case-insensitively and marks the matched span", () => {
  const matches = searchSettings("  ШРИФТ ");
  expect(matches.map((match) => match.entry.label)).toEqual([FONT_FAMILY_LABEL, FONT_SIZE_LABEL]);
  const size = matches[1]!;
  expect(size.entry.label.slice(size.start, size.end).toLocaleLowerCase("ru-RU")).toBe("шрифт");
  expect(searchSettings("   ")).toEqual([]);
  expect(searchSettings("несуществующее")).toEqual([]);
});

test("titles that start with the query rank before those that only contain it", () => {
  const entries = [
    { id: "a", section: "general" as const, label: "Размер шрифта" },
    { id: "b", section: "general" as const, label: "Шрифт" },
  ];
  expect(searchSettings("шрифт", entries).map((match) => match.entry.id)).toEqual(["b", "a"]);
});

test("every setting id is unique and a notification kind falls back to its master switch", () => {
  const ids = SETTING_ENTRIES.map((entry) => entry.id);
  expect(new Set(ids).size).toBe(ids.length);
  const kind = SETTING_ENTRIES.find((entry) => entry.id === NOTIFICATION_KIND_ROWS[0]!.key);
  expect(kind?.fallback).toBe(NOTIFICATIONS_ROW.key);
});

test("revealing a setting opens its section and closing clears the target", () => {
  const ui = new UiStore(createIpcClient(contract, createFakeBridge<Contract>()));
  ui.revealPreference("appearance", SETTING_IDS.fontSize);
  expect(ui.preferences).toBe("appearance");
  expect(ui.preferenceTarget).toBe(SETTING_IDS.fontSize);
  ui.closePreferences();
  expect(ui.preferenceTarget).toBeNull();
});
