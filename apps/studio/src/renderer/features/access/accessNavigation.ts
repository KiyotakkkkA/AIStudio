import { ROUTES } from "../../app/routes";
import type { AccessTab } from "../permissions/PermissionsStore";

export const ACCESS_TAB_ORDER: readonly AccessTab[] = ["folders", "sites", "apps", "rules"];
export const BROWSER_SITE_ACCESS_URL = `${ROUTES.permissions}?tab=sites&fromPreference=browser`;

export function requestedAccessTab(params: URLSearchParams): AccessTab | null {
  return ACCESS_TAB_ORDER.find((tab) => tab === params.get("tab")) ?? null;
}

export function fromBrowserPreferences(params: URLSearchParams): boolean {
  return params.get("fromPreference") === "browser";
}

export const DEVICE_APP_ACCESS_URL = `${ROUTES.permissions}?tab=apps&fromPreference=device`;

export function fromDevicePreferences(params: URLSearchParams): boolean {
  return params.get("fromPreference") === "device";
}
