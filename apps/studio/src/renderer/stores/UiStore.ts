import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract } from "@zvs/shared";
import { DEFAULT_ROUTE, isRoutePath, type RoutePath } from "../app/routes";

const RAIL_COLLAPSED_KEY = "ui.rail.collapsed";
const LAST_ROUTE_KEY = "ui.route.last";
const THEME_KEY = "ui.theme";
const RAIL_THEME_PICKER_KEY = "ui.rail.themepicker";
const FONT_FAMILY_KEY = "ui.font.family";
const FONT_SIZE_KEY = "ui.font.size";

export const THEME_MODES = ["system", "light", "dark"] as const;
export type ThemeMode = (typeof THEME_MODES)[number];

function isThemeMode(value: unknown): value is ThemeMode {
  return typeof value === "string" && (THEME_MODES as readonly string[]).includes(value);
}

export const FONT_FAMILIES = ["onest", "system", "mono"] as const;
export type FontFamily = (typeof FONT_FAMILIES)[number];

export const FONT_SIZES = [12, 13, 14, 15, 16] as const;
export type FontSize = (typeof FONT_SIZES)[number];
export const DEFAULT_FONT_SIZE: FontSize = 13;

export const PREFERENCE_SECTIONS = [
  "general",
  "profile",
  "appearance",
  "notifications",
  "memory",
  "about",
] as const;
export type PreferenceSection = (typeof PREFERENCE_SECTIONS)[number];

function isFontFamily(value: unknown): value is FontFamily {
  return typeof value === "string" && (FONT_FAMILIES as readonly string[]).includes(value);
}

function isFontSize(value: unknown): value is FontSize {
  return typeof value === "number" && (FONT_SIZES as readonly number[]).includes(value);
}

export class UiStore {
  railCollapsed = false;
  activeRoute: RoutePath = DEFAULT_ROUTE;
  theme: ThemeMode = "system";
  railThemePicker = true;
  fontFamily: FontFamily = "onest";
  fontSize: FontSize = DEFAULT_FONT_SIZE;
  preferences: PreferenceSection | null = null;
  preferenceTarget: string | null = null;
  restored = false;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<UiStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  get railWidth(): number {
    return this.railCollapsed ? 64 : 244;
  }

  async restore(): Promise<void> {
    const [collapsed, route, theme, picker, family, size] = await Promise.all([
      this.read(RAIL_COLLAPSED_KEY),
      this.read(LAST_ROUTE_KEY),
      this.read(THEME_KEY),
      this.read(RAIL_THEME_PICKER_KEY),
      this.read(FONT_FAMILY_KEY),
      this.read(FONT_SIZE_KEY),
    ]);
    runInAction(() => {
      if (isThemeMode(theme)) this.theme = theme;
      if (typeof picker === "boolean") this.railThemePicker = picker;
      if (isFontFamily(family)) this.fontFamily = family;
      if (isFontSize(size)) this.fontSize = size;
      if (typeof collapsed === "boolean") this.railCollapsed = collapsed;
      if (isRoutePath(route)) this.activeRoute = route;
      this.restored = true;
    });
  }

  toggleRail(): void {
    this.setRailCollapsed(!this.railCollapsed);
  }

  setRailCollapsed(collapsed: boolean): void {
    if (this.railCollapsed === collapsed) return;
    this.railCollapsed = collapsed;
    void this.write(RAIL_COLLAPSED_KEY, collapsed);
  }

  setTheme(theme: ThemeMode): void {
    if (this.theme === theme) return;
    this.theme = theme;
    void this.write(THEME_KEY, theme);
  }

  setRailThemePicker(visible: boolean): void {
    if (this.railThemePicker === visible) return;
    this.railThemePicker = visible;
    void this.write(RAIL_THEME_PICKER_KEY, visible);
  }

  setFontFamily(family: FontFamily): void {
    if (this.fontFamily === family) return;
    this.fontFamily = family;
    void this.write(FONT_FAMILY_KEY, family);
  }

  setFontSize(size: FontSize): void {
    if (this.fontSize === size) return;
    this.fontSize = size;
    void this.write(FONT_SIZE_KEY, size);
  }

  openPreferences(section: PreferenceSection = "appearance"): void {
    this.preferences = section;
  }

  revealPreference(section: PreferenceSection, target: string): void {
    this.preferences = section;
    this.preferenceTarget = target;
  }

  clearPreferenceTarget(): void {
    this.preferenceTarget = null;
  }

  closePreferences(): void {
    this.preferences = null;
    this.preferenceTarget = null;
  }

  setActiveRoute(route: string): void {
    if (!isRoutePath(route) || this.activeRoute === route) return;
    this.activeRoute = route;
    void this.write(LAST_ROUTE_KEY, route);
  }

  private async read(key: string): Promise<unknown> {
    try {
      const stored = await this.ipc.call("settings.get", { key });
      return stored.value;
    } catch {
      return undefined;
    }
  }

  private async write(key: string, value: boolean | string | number): Promise<void> {
    try {
      await this.ipc.call("settings.set", { key, value });
    } catch {
      return;
    }
  }
}
