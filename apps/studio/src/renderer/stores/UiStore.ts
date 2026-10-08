import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract } from "@zvs/shared";
import { DEFAULT_ROUTE, isRoutePath, type RoutePath } from "../app/routes";

const RAIL_COLLAPSED_KEY = "ui.rail.collapsed";
const LAST_ROUTE_KEY = "ui.route.last";
const THEME_KEY = "ui.theme";

export const THEME_MODES = ["system", "light", "dark"] as const;
export type ThemeMode = (typeof THEME_MODES)[number];

function isThemeMode(value: unknown): value is ThemeMode {
  return typeof value === "string" && (THEME_MODES as readonly string[]).includes(value);
}

export const VIEWER_DOCK_MIN_WIDTH = 320;
export const VIEWER_DOCK_MAX_WIDTH = 720;
export const VIEWER_DOCK_DEFAULT_WIDTH = 420;

export class UiStore {
  railCollapsed = false;
  activeRoute: RoutePath = DEFAULT_ROUTE;
  viewerDockWidth = VIEWER_DOCK_DEFAULT_WIDTH;
  theme: ThemeMode = "system";
  restored = false;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<UiStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  get railWidth(): number {
    return this.railCollapsed ? 64 : 244;
  }

  async restore(): Promise<void> {
    const [collapsed, route, theme] = await Promise.all([
      this.read(RAIL_COLLAPSED_KEY),
      this.read(LAST_ROUTE_KEY),
      this.read(THEME_KEY),
    ]);
    runInAction(() => {
      if (isThemeMode(theme)) this.theme = theme;
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

  setActiveRoute(route: string): void {
    if (!isRoutePath(route) || this.activeRoute === route) return;
    this.activeRoute = route;
    void this.write(LAST_ROUTE_KEY, route);
  }

  setViewerDockWidth(width: number): void {
    const clamped = Math.min(
      VIEWER_DOCK_MAX_WIDTH,
      Math.max(VIEWER_DOCK_MIN_WIDTH, Math.round(width)),
    );
    this.viewerDockWidth = clamped;
  }

  private async read(key: string): Promise<unknown> {
    try {
      const stored = await this.ipc.call("settings.get", { key });
      return stored.value;
    } catch {
      return undefined;
    }
  }

  private async write(key: string, value: boolean | string): Promise<void> {
    try {
      await this.ipc.call("settings.set", { key, value });
    } catch {
      return;
    }
  }
}
