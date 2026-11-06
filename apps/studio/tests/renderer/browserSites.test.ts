import { describe, expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  contract,
  type BrowserSiteRulesDto,
  type BrowserSitesDto,
  type Contract,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import BrowserSitesStore, {
  DEFAULT_SITE_RULES,
} from "../../src/renderer/features/browser/BrowserSitesStore.ts";
import {
  SITE_INVALID,
  SITES_LOAD_FAILED,
  SITES_SAVE_FAILED,
  siteHost,
  siteRemoveConsequence,
} from "../../src/renderer/features/browser/browserSitesCopy.ts";

const SHOP = "https://shop.acme.io";

function setup(initial: BrowserSitesDto = { defaults: { ...DEFAULT_SITE_RULES }, sites: [] }) {
  let stored = initial;
  const bridge = createFakeBridge<Contract>()
    .handle("browserSites.get", () => stored)
    .handle("browserSites.setDefault", ({ action, decision }) => {
      stored = { ...stored, defaults: { ...stored.defaults, [action]: decision } };
      return stored;
    })
    .handle("browserSites.put", ({ site, rules }) => {
      const origin = site.startsWith("http") ? site : `https://${site}`;
      const others = stored.sites.filter((entry) => entry.origin !== origin);
      stored = {
        ...stored,
        sites:
          Object.keys(rules).length === 0
            ? others
            : [...others, { origin, rules, updatedAt: 1_000 }],
      };
      return stored;
    })
    .handle("browserSites.remove", ({ origin }) => {
      stored = { ...stored, sites: stored.sites.filter((entry) => entry.origin !== origin) };
      return stored;
    });
  const store = new BrowserSitesStore(createIpcClient(contract, bridge));
  return {
    store,
    bridge,
    puts: () => bridge.calls.filter((call) => call.channel === "browserSites.put"),
  };
}

describe("BrowserSitesStore", () => {
  test("changes a default decision", async () => {
    const { store } = setup();
    await store.load();
    await store.setDefault("download", "block");
    expect(store.sites.defaults.download).toBe("block");
  });

  test("adds a site from the draft and closes the form", async () => {
    const { store } = setup();
    await store.load();
    store.startAdding();
    expect(store.canAdd).toBe(false);
    store.setDraftSite("  shop.acme.io ");
    store.setDraftRule("click", "allow");
    store.setDraftRule("type", "block");
    store.setDraftRule("type", null);
    expect(store.canAdd).toBe(true);
    await store.addDraft();
    expect(store.adding).toBe(false);
    expect(store.sites.sites).toEqual([
      { origin: SHOP, rules: { click: "allow" }, updatedAt: 1_000 },
    ]);
  });

  test("a rule set back to the default is dropped from the site", async () => {
    const rules: BrowserSiteRulesDto = { click: "allow", submit: "block" };
    const { store, puts } = setup({
      defaults: { ...DEFAULT_SITE_RULES },
      sites: [{ origin: SHOP, rules, updatedAt: 1 }],
    });
    await store.load();
    await store.setRule(SHOP, "submit", null);
    expect(puts().at(-1)?.payload).toEqual({ site: SHOP, rules: { click: "allow" } });
    await store.remove(SHOP);
    expect(store.sites.sites).toEqual([]);
  });

  test("keeps the form open with copy when the host refuses the site", async () => {
    const { store, bridge } = setup();
    bridge.fail("browserSites.put", AppErrorCode.VALIDATION_FAILED, "not a site");
    store.startAdding();
    store.setDraftSite("file:///C:/");
    await store.addDraft();
    expect(store.adding).toBe(true);
    expect(store.error).toBe(SITE_INVALID);
    bridge.fail("browserSites.setDefault", AppErrorCode.DB_ERROR, "db");
    await store.setDefault("browse", "ask");
    expect(store.error).toBe(SITES_SAVE_FAILED);
    bridge.fail("browserSites.get", AppErrorCode.DB_ERROR, "db");
    await store.load();
    expect(store.error).toBe(SITES_LOAD_FAILED);
    expect(store.loaded).toBe(true);
  });

  test("names the site in the removal confirmation", () => {
    expect(siteHost("https://shop.acme.io:8443")).toBe("shop.acme.io:8443");
    expect(siteRemoveConsequence("shop.acme.io")).toContain("shop.acme.io");
  });
});
