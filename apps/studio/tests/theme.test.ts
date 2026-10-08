import assert from "node:assert/strict";
import { test } from "vitest";
import {
  applyThemeSource,
  THEME_SETTING_KEY,
  type ThemeTarget,
} from "../src/host/platform/theme.ts";
import { createSettingHandlers } from "../src/host/ipc/settings.ts";
import type { SettingService } from "../src/host/services/SettingService.ts";

test("only known theme modes reach the platform colour scheme", () => {
  const target: ThemeTarget = { themeSource: "system" };
  applyThemeSource("dark", target);
  assert.equal(target.themeSource, "dark");
  applyThemeSource("sepia", target);
  applyThemeSource(undefined, target);
  applyThemeSource(1, target);
  assert.equal(target.themeSource, "dark");
  applyThemeSource("light", target);
  assert.equal(target.themeSource, "light");
});

test("saving the theme setting mirrors it to the platform, other keys do not", async () => {
  const applied: unknown[] = [];
  const settings = {
    set: (key: string, value: unknown) => ({ key, value, updatedAt: 1 }),
  } as unknown as SettingService;
  const handlers = createSettingHandlers(settings, undefined, (value) => applied.push(value));

  await handlers["settings.set"]({ key: THEME_SETTING_KEY, value: "light" });
  await handlers["settings.set"]({ key: "ui.rail.collapsed", value: true });

  assert.deepEqual(applied, ["light"]);
});
