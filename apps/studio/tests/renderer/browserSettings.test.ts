import { describe, expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { contract, type BrowserSettingsDto, type Contract } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import BrowserSettingsStore, {
  DEFAULT_BROWSER_SETTINGS,
} from "../../src/renderer/features/browser/BrowserSettingsStore.ts";
import { BROWSER_SAVE_FAILED } from "../../src/renderer/features/browser/browserSettingsCopy.ts";
import { BROWSER_NAV_ID, visibleNavGroups } from "../../src/renderer/app/navigation.ts";

function setup(initial: BrowserSettingsDto = DEFAULT_BROWSER_SETTINGS) {
  let stored = initial;
  let refuse = false;
  const saved: BrowserSettingsDto[] = [];
  const bridge = createFakeBridge<Contract>()
    .handle("browserSettings.get", () => stored)
    .handle("browserSettings.set", (settings) => {
      if (refuse) throw new Error("refused");
      saved.push(settings);
      stored = settings;
      return settings;
    });
  const store = new BrowserSettingsStore(createIpcClient(contract, bridge));
  return {
    store,
    saved,
    refuse: () => {
      refuse = true;
    },
  };
}

describe("BrowserSettingsStore", () => {
  test("loads the stored settings", async () => {
    const { store } = setup({ ...DEFAULT_BROWSER_SETTINGS, enabled: false, fullUrl: true });
    expect(store.enabled).toBe(true);
    await store.load();
    expect(store.loaded).toBe(true);
    expect(store.enabled).toBe(false);
    expect(store.settings.fullUrl).toBe(true);
  });

  test("saves a change as the whole settings object", async () => {
    const { store, saved } = setup();
    await store.load();
    await store.update({ localLinks: "system" });
    expect(saved).toEqual([{ ...DEFAULT_BROWSER_SETTINGS, localLinks: "system" }]);
    expect(store.settings.localLinks).toBe("system");
  });

  test("restores the previous value when saving fails", async () => {
    const { store, refuse } = setup();
    await store.load();
    refuse();
    await store.update({ enabled: false });
    expect(store.enabled).toBe(true);
    expect(store.error).toBe(BROWSER_SAVE_FAILED);
    store.dismissError();
    expect(store.error).toBeNull();
  });
});

describe("visibleNavGroups", () => {
  const ids = (browser: boolean) =>
    visibleNavGroups(browser).flatMap((group) => group.items.map((item) => item.id));

  test("drops the browser item only while the browser is off", () => {
    expect(ids(true)).toContain(BROWSER_NAV_ID);
    expect(ids(false)).not.toContain(BROWSER_NAV_ID);
    expect(ids(false)).toHaveLength(ids(true).length - 1);
  });
});
