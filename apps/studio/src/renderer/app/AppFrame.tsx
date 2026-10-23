import { observer } from "mobx-react-lite";
import { useCallback, useEffect, useRef } from "react";
import { Outlet } from "react-router-dom";
import { useAppNavigation } from "../hooks/useAppNavigation";
import { useStore } from "../stores/useStore";
import NavRail from "../ui/organisms/NavRail";
import { NAV_GROUPS, RAIL_IDENTITY } from "./navigation";
import type { NavItemModel } from "../ui/organisms/NavRailTypes";
import AppShell from "../ui/templates/AppShell";
import { HostEvent } from "@zvs/shared";
import PreferencesDialog from "../features/preferences/PreferencesDialog";
import { fontStack } from "../features/preferences/preferencesPresentation";

function AppFrame() {
  const { ui } = useStore();
  const navigation = useAppNavigation();
  const restoredOnce = useRef(false);

  useEffect(
    () =>
      window.zvs.subscribe((payload) => {
        const parsed = HostEvent.safeParse(payload);
        if (!parsed.success || parsed.data.type !== "step") return;
        const { domain, path } = parsed.data.step;
        if (domain === "navigation" && (path === "/browser" || path === "/providers"))
          navigation.go(path);
      }),
    [navigation],
  );

  useEffect(() => {
    if (!ui.restored || restoredOnce.current) return;
    restoredOnce.current = true;
    navigation.replace(ui.activeRoute);
  }, [ui.restored, ui.activeRoute, navigation]);

  useEffect(() => {
    const root = document.documentElement;
    if (ui.theme === "system") delete root.dataset.theme;
    else root.dataset.theme = ui.theme;
  }, [ui.theme]);

  useEffect(() => {
    const stack = fontStack(ui.fontFamily);
    const root = document.documentElement;
    if (stack === null) root.style.removeProperty("--font-sans");
    else root.style.setProperty("--font-sans", stack);
  }, [ui.fontFamily]);

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
      if (item.path !== undefined) navigation.go(item.path);
    },
    [navigation],
  );

  const openPreferences = useCallback(() => {
    ui.openPreferences();
  }, [ui]);

  return (
    <AppShell
      rail={
        <NavRail
          groups={NAV_GROUPS}
          identity={RAIL_IDENTITY}
          collapsed={ui.railCollapsed}
          theme={ui.theme}
          themePicker={ui.railThemePicker}
          isActive={isActive}
          onSelect={select}
          onToggleCollapse={ui.toggleRail}
          onOpenIdentity={openPreferences}
          onThemeChange={ui.setTheme}
        />
      }
    >
      <Outlet />
      <PreferencesDialog />
    </AppShell>
  );
}

export default observer(AppFrame);
