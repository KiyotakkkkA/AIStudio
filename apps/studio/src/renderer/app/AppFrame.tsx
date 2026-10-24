import { observer } from "mobx-react-lite";
import { useCallback, useEffect, useMemo, useRef } from "react";
import { MotionConfig } from "motion/react";
import { Outlet } from "react-router-dom";
import { useAppNavigation } from "../hooks/useAppNavigation";
import { useStore } from "../stores/useStore";
import NavRail from "../ui/organisms/NavRail";
import { NAV_GROUPS, RAIL_IDENTITY } from "./navigation";
import type { NavItemModel } from "../ui/organisms/NavRailTypes";
import AppShell from "../ui/templates/AppShell";
import { ConversationId, HostEvent, NAVIGATION_STEP_DOMAIN } from "@zvs/shared";
import { ROUTES } from "./routes";
import PreferencesDialog from "../features/preferences/PreferencesDialog";
import { fontStack } from "../features/preferences/preferencesPresentation";
import { FxContext } from "../ui/motion/fx";

const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

function AppFrame() {
  const { ui, chat, appSettings } = useStore();
  const navigation = useAppNavigation();
  const restoredOnce = useRef(false);

  const follow = useCallback(
    (path: unknown, conversationId?: unknown) => {
      if (path === ROUTES.browser || path === ROUTES.providers) navigation.go(path);
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
    [navigation, ui, chat],
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
    void appSettings.takePendingTarget().then((target) => {
      if (target?.kind === "conversation") follow(ROUTES.chat, target.conversationId);
      if (target?.kind === "downloads") follow(ROUTES.downloads);
      if (target?.kind === "scenarios") follow(ROUTES.scenarios);
    });
  }, [ui.restored, ui.activeRoute, navigation, appSettings, follow]);

  useEffect(() => {
    const root = document.documentElement;
    if (ui.theme === "system") delete root.dataset.theme;
    else root.dataset.theme = ui.theme;
  }, [ui.theme]);

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const query = window.matchMedia(REDUCED_MOTION_QUERY);
    const sync = () => ui.setOsReducedMotion(query.matches);
    sync();
    query.addEventListener("change", sync);
    return () => query.removeEventListener("change", sync);
  }, [ui]);

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.motion = ui.motion;
    root.dataset.effects = ui.effects;
  }, [ui.motion, ui.effects]);

  const fx = useMemo(() => ({ motion: ui.motion, effects: ui.effects }), [ui.motion, ui.effects]);

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
    <FxContext value={fx}>
      <MotionConfig reducedMotion={ui.motion === "none" ? "always" : "never"}>
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
      </MotionConfig>
    </FxContext>
  );
}

export default observer(AppFrame);
