import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract } from "@zvs/shared";
import { DEFAULT_ROUTE, isRoutePath, type RoutePath } from "../app/routes";

const RAIL_COLLAPSED_KEY = "ui.rail.collapsed";
const LAST_ROUTE_KEY = "ui.route.last";

export const VIEWER_DOCK_MIN_WIDTH = 320;
export const VIEWER_DOCK_MAX_WIDTH = 720;
export const VIEWER_DOCK_DEFAULT_WIDTH = 420;

export class UiStore {
  railCollapsed = false;
  activeRoute: RoutePath = DEFAULT_ROUTE;
  viewerDockWidth = VIEWER_DOCK_DEFAULT_WIDTH;
  restored = false;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<UiStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  get railWidth(): number {
    return this.railCollapsed ? 64 : 248;
  }

  async restore(): Promise<void> {
    const [collapsed, route] = await Promise.all([
      this.read(RAIL_COLLAPSED_KEY),
      this.read(LAST_ROUTE_KEY),
    ]);
    runInAction(() => {
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
