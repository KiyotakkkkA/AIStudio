import { observer } from "mobx-react-lite";
import { useCallback, useEffect, useMemo, useRef } from "react";
import { MotionConfig } from "motion/react";
import { Outlet } from "react-router-dom";
import { useAppNavigation } from "../hooks/useAppNavigation";
import { useStore } from "../stores/useStore";
import NavRail from "../ui/organisms/NavRail";
import { RAIL_IDENTITY, visibleNavGroups } from "./navigation";
import type { NavItemModel } from "../ui/organisms/NavRailTypes";
import AppShell from "../ui/templates/AppShell";
import { ConversationId, HostEvent, NAVIGATION_STEP_DOMAIN } from "@zvs/shared";
import { ROUTES } from "./routes";
import PreferencesDialog from "../features/preferences/PreferencesDialog";
import { FxContext } from "../ui/motion/fx";

function AppFrame() {
  const { ui, chat, appSettings, browserSettings } = useStore();
  const navigation = useAppNavigation();
  const restoredOnce = useRef(false);

  const follow = useCallback(
    (path: unknown, conversationId?: unknown) => {
      if (path === ROUTES.browser && browserSettings.enabled) navigation.go(path);
      if (path === ROUTES.providers) navigation.go(path);
      if (path === ROUTES.downloads) {
        ui.closePreferences();
        navigation.go(path);
      }
      if (path === ROUTES.chat) {
        ui.closePreferences();
        navigation.go(path);
        const conversation = ConversationId.safeParse(conversationId);
        if (conversation.success) void chat.open(conversation.data);
      }
    },
    [navigation, ui, chat, browserSettings],
  );

  useEffect(
    () =>
      window.zvs.subscribe((payload) => {
        const parsed = HostEvent.safeParse(payload);
        if (!parsed.success || parsed.data.type !== "step") return;
        const { domain, path, conversationId } = parsed.data.step;
        if (domain === NAVIGATION_STEP_DOMAIN) follow(path, conversationId);
      }),
    [follow],
  );

  useEffect(() => {
    if (!ui.restored || restoredOnce.current) return;
    restoredOnce.current = true;
    navigation.replace(ui.activeRoute);
    void appSettings.load();
    void browserSettings.load();
    void appSettings.takePendingTarget().then((target) => {
      if (target?.kind === "conversation") follow(ROUTES.chat, target.conversationId);
      if (target?.kind === "downloads") follow(ROUTES.downloads);
      if (target?.kind === "scenarios") follow(ROUTES.scenarios);
    });
  }, [ui.restored, ui.activeRoute, navigation, appSettings, browserSettings, follow]);

  const fx = useMemo(() => ({ motion: ui.motion, effects: ui.effects }), [ui.motion, ui.effects]);

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
    <FxContext value={fx}>
      <MotionConfig reducedMotion={ui.motion === "none" ? "always" : "never"}>
        <AppShell
          rail={
            <NavRail
              groups={visibleNavGroups(browserSettings.enabled)}
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
      </MotionConfig>
    </FxContext>
  );
}

export default observer(AppFrame);
