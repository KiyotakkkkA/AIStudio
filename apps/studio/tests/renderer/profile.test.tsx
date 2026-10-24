import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { AppErrorCode, contract, type Contract, type Json } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createFakeClock } from "../../../../test/helpers/fakeClock.ts";
import ProfileStore, {
  PROFILE_KEYS,
  REPLY_STYLES,
  TOOL_CALLS_DEFAULT,
  TOOL_CALLS_MAX,
  parseToolCalls,
} from "../../src/renderer/features/preferences/ProfileStore.ts";
import {
  STYLE_DESCRIPTIONS,
  STYLE_OPTIONS,
} from "../../src/renderer/features/preferences/profileCopy.ts";
import { SETTING_SAVE_FAILED } from "../../src/renderer/features/preferences/preferencesPresentation.ts";
import { composerCounter } from "../../src/renderer/ui/molecules/TextComposer.tsx";
import { searchSettings } from "../../src/renderer/features/preferences/settingsIndex.ts";

function profile(seed: Record<string, Json> = {}) {
  const stored = new Map<string, Json>(Object.entries(seed));
  const clock = createFakeClock();
  const bridge = createFakeBridge<Contract>()
    .handle("settings.get", ({ key }) => ({ key, value: stored.get(key) }))
    .handle("settings.set", ({ key, value }) => {
      stored.set(key, value);
      return { key, value, updatedAt: clock.advance(1_000) };
    });
  return { bridge, stored, store: new ProfileStore(createIpcClient(contract, bridge)) };
}

test("four preset styles and a custom one are offered, each described", () => {
  expect(STYLE_OPTIONS.map((option) => option.value)).toEqual([...REPLY_STYLES]);
  expect(REPLY_STYLES).toHaveLength(5);
  expect(REPLY_STYLES.at(-1)).toBe("custom");
  for (const style of REPLY_STYLES) expect(STYLE_DESCRIPTIONS[style]).toBeTruthy();
});

test("the profile loads its defaults and ignores malformed stored values", async () => {
  const { store } = profile({
    [PROFILE_KEYS.style]: "loud",
    [PROFILE_KEYS.about]: 3,
    [PROFILE_KEYS.toolCalls]: 900,
  });
  await store.load();
  expect(store.style).toBe("standard");
  expect(store.about).toBe("");
  expect(store.toolCalls).toBe(TOOL_CALLS_DEFAULT);
  expect(store.toolCallsDraft).toBe("15");
});

test("a style switch persists at once and a refused one rolls back", async () => {
  const { bridge, stored, store } = profile({ [PROFILE_KEYS.style]: "concise" });
  await store.load();
  expect(store.style).toBe("concise");
  await store.setStyle("formal");
  expect(stored.get(PROFILE_KEYS.style)).toBe("formal");

  bridge.fail("settings.set", AppErrorCode.DB_ERROR, "x");
  await store.setStyle("custom");
  expect(store.style).toBe("formal");
  expect(store.errors.style).toBe(SETTING_SAVE_FAILED);
  expect(store.pending).toEqual({});
});

test("text fields save a trimmed draft only when it changed and can be reset", async () => {
  const { bridge, stored, store } = profile({ [PROFILE_KEYS.about]: "Rust" });
  await store.load();
  expect(store.aboutDirty).toBe(false);
  store.setAboutDraft("  Rust  ");
  expect(store.aboutDirty).toBe(false);
  store.setAboutDraft("Rust и Go ");
  expect(store.aboutDirty).toBe(true);
  await store.saveAbout();
  expect(stored.get(PROFILE_KEYS.about)).toBe("Rust и Go");
  expect([store.about, store.aboutDraft, store.aboutDirty]).toEqual([
    "Rust и Go",
    "Rust и Go",
    false,
  ]);

  store.setCustomDraft("По пунктам");
  store.resetCustom();
  expect(store.customDraft).toBe("");
  store.setCustomDraft("По пунктам");
  await store.saveCustom();
  expect(stored.get(PROFILE_KEYS.custom)).toBe("По пунктам");

  const calls = bridge.calls.length;
  await store.saveCustom();
  expect(bridge.calls).toHaveLength(calls);
});

test("the tool call limit commits a clamped whole number and reverts anything else", async () => {
  expect(parseToolCalls("20")).toBe(20);
  expect(parseToolCalls("0")).toBe(1);
  expect(parseToolCalls("500")).toBe(TOOL_CALLS_MAX);
  expect(parseToolCalls("2.5")).toBeNull();
  expect(parseToolCalls("")).toBeNull();

  const { stored, store } = profile();
  await store.load();
  store.setToolCallsDraft("80");
  await store.commitToolCalls();
  expect(stored.get(PROFILE_KEYS.toolCalls)).toBe(TOOL_CALLS_MAX);
  expect(store.toolCallsDraft).toBe(String(TOOL_CALLS_MAX));

  store.setToolCallsDraft("abc");
  await store.commitToolCalls();
  expect(store.toolCallsDraft).toBe(String(TOOL_CALLS_MAX));
  expect(store.toolCalls).toBe(TOOL_CALLS_MAX);
});

test("profile rows are found by search and counters use the Russian locale", () => {
  expect(searchSettings("стиль").map((match) => match.entry.section)).toContain("profile");
  expect(searchSettings("вызовов").map((match) => match.entry.id)).toEqual([
    PROFILE_KEYS.toolCalls,
  ]);
  expect(composerCounter(1200, 4000)).toBe(
    `${(1200).toLocaleString("ru-RU")} / ${(4000).toLocaleString("ru-RU")}`,
  );
});
