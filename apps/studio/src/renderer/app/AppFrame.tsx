import { observer } from "mobx-react-lite";
import { useCallback, useEffect, useRef } from "react";
import { Outlet } from "react-router-dom";
import { useAppNavigation } from "../hooks/useAppNavigation";
import { useStore } from "../stores/useStore";
import NavRail from "../ui/organisms/NavRail";
import { BROWSER_NAV_ID, NAV_GROUPS, RAIL_IDENTITY } from "./navigation";
import type { NavItemModel } from "../ui/organisms/NavRailTypes";
import AppShell from "../ui/templates/AppShell";
import { ROUTES } from "./routes";

const APP_VERSION = "v0.1.0 · local";

function openBrowserWindow(): void {
  console.info("[renderer] Окно браузера появится в TASK_045");
}

function AppFrame() {
  const { ui } = useStore();
  const navigation = useAppNavigation();
  const restoredOnce = useRef(false);

  useEffect(() => {
    if (!ui.restored || restoredOnce.current) return;
    restoredOnce.current = true;
    navigation.replace(ui.activeRoute);
  }, [ui.restored, ui.activeRoute, navigation]);

  useEffect(() => {
    if (!restoredOnce.current) return;
    ui.setActiveRoute(navigation.pathname);
  }, [ui, navigation.pathname]);

  const isActive = useCallback(
    (item: NavItemModel) => item.path !== undefined && navigation.isActive(item.path),
    [navigation],
  );

  const select = useCallback(
    (item: NavItemModel) => {
      if (item.id === BROWSER_NAV_ID) {
        openBrowserWindow();
        return;
      }
      if (item.path !== undefined) navigation.go(item.path);
    },
    [navigation],
  );

  const openSettings = useCallback(() => {
    navigation.go(ROUTES.settings);
  }, [navigation]);

  return (
    <AppShell
      rail={
        <NavRail
          groups={NAV_GROUPS}
          identity={RAIL_IDENTITY}
          version={APP_VERSION}
          collapsed={ui.railCollapsed}
          isActive={isActive}
          onSelect={select}
          onToggleCollapse={ui.toggleRail}
          onOpenIdentity={openSettings}
        />
      }
    >
      <Outlet />
    </AppShell>
  );
}

export default observer(AppFrame);
