import { describe, expect, test } from "vitest";
import { ROUTES } from "../../src/renderer/app/routes";
import {
  BROWSER_SITE_ACCESS_URL,
  DEVICE_APP_ACCESS_URL,
  fromDevicePreferences,
  fromBrowserPreferences,
  requestedAccessTab,
} from "../../src/renderer/features/access/accessNavigation";

describe("access navigation", () => {
  test("opens site permissions with a browser preferences return target", () => {
    const url = new URL(BROWSER_SITE_ACCESS_URL, "https://studio.local");
    expect(url.pathname).toBe(ROUTES.permissions);
    expect(requestedAccessTab(url.searchParams)).toBe("sites");
    expect(fromBrowserPreferences(url.searchParams)).toBe(true);
  });

  test("opens app permissions with a device preferences return target", () => {
    const url = new URL(DEVICE_APP_ACCESS_URL, "https://studio.local");
    expect(url.pathname).toBe(ROUTES.permissions);
    expect(requestedAccessTab(url.searchParams)).toBe("apps");
    expect(fromDevicePreferences(url.searchParams)).toBe(true);
    expect(fromBrowserPreferences(url.searchParams)).toBe(false);
    const browser = new URL(BROWSER_SITE_ACCESS_URL, "https://studio.local");
    expect(fromDevicePreferences(browser.searchParams)).toBe(false);
  });

  test("keeps the return target when switching access tabs", () => {
    const params = new URL(BROWSER_SITE_ACCESS_URL, "https://studio.local").searchParams;
    params.set("tab", "folders");
    expect(requestedAccessTab(params)).toBe("folders");
    expect(fromBrowserPreferences(params)).toBe(true);
  });

  test("ignores missing and unsupported tabs and return targets", () => {
    for (const query of ["", "tab=unknown", "tab=toString", "tab=tools"]) {
      expect(requestedAccessTab(new URLSearchParams(query))).toBeNull();
    }
    for (const query of ["", "fromPreference=device", "fromPreference=https://example.com"]) {
      expect(fromBrowserPreferences(new URLSearchParams(query))).toBe(false);
    }
  });
});
