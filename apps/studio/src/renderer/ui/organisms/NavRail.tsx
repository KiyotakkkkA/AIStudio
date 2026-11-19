import {
  mdiAccountCircleOutline,
  mdiMonitor,
  mdiWeatherNight,
  mdiWhiteBalanceSunny,
} from "@mdi/js";
import { Dropdown, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { motion } from "motion/react";
import AppLogo from "../atoms/AppLogo";
import Icon from "../atoms/Icon";
import { ActionMenu, ActionMenuItem } from "../molecules/ActionMenu";
import NavRailItem from "../molecules/NavRailItem";
import type { NavGroupModel, NavItemModel, RailIdentity, RailTheme } from "./NavRailTypes";
import { useFx, useTransition } from "../motion/fx";

const THEME_OPTIONS: readonly { value: RailTheme; label: string; icon: string }[] = [
  { value: "light", label: "Светлая тема", icon: mdiWhiteBalanceSunny },
  { value: "dark", label: "Тёмная тема", icon: mdiWeatherNight },
  { value: "system", label: "Как в системе", icon: mdiMonitor },
];

export interface NavRailProps {
  readonly groups: readonly NavGroupModel[];
  readonly identity: RailIdentity;
  readonly onOpenProfile: () => void;
  readonly collapsed: boolean;
  readonly theme: RailTheme;
  readonly themePicker: boolean;
  readonly isActive: (item: NavItemModel) => boolean;
  readonly onSelect: (item: NavItemModel) => void;
  readonly onThemeChange: (theme: RailTheme) => void;
}

export default function NavRail({
  groups,
  identity,
  onOpenProfile,
  collapsed,
  theme,
  themePicker,
  isActive,
  onSelect,
  onThemeChange,
}: NavRailProps) {
  const { motion: level } = useFx();
  const layout = useTransition("layout");
  return (
    <motion.aside
      initial={false}
      animate={{ width: collapsed ? 64 : 244 }}
      transition={level === "none" ? { duration: 0 } : layout}
      className="flex h-full flex-none flex-col overflow-hidden bg-main-950"
    >
      <div
        className={`flex h-16 flex-none items-center gap-2.5 ${
          collapsed ? "justify-center px-0" : "pr-2 pl-4.5"
        }`}
      >
        <AppLogo size={32} className="drop-shadow-[0_2px_6px_rgb(180_80_20/0.25)]" />
        {collapsed ? null : (
          <span className="min-w-0 flex-1 truncate text-[14px] font-semibold tracking-[-0.01em] text-main-50">
            ZVS AI Studio
          </span>
        )}
      </div>

      <ScrollArea className={`min-h-0 flex-1 py-1 ${collapsed ? "px-2.5" : "px-3"}`}>
        <nav className="flex flex-col gap-0.5">
          {groups.map((group, index) => (
            <div key={group.id} className="flex flex-col gap-0.5">
              {collapsed ? (
                index === 0 ? null : (
                  <div className="m-2  h-px bg-main-750" />
                )
              ) : (
                <div
                  className={`px-2.5 pb-1.5 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase ${
                    index === 0 ? "pt-1" : "pt-5"
                  }`}
                >
                  {group.label}
                </div>
              )}
              {group.items.map((item) => (
                <NavRailItem
                  key={item.id}
                  item={item}
                  active={isActive(item)}
                  collapsed={collapsed}
                  onSelect={onSelect}
                />
              ))}
            </div>
          ))}
        </nav>
      </ScrollArea>

      <div
        className={`flex flex-none flex-col gap-1 rounded-card border border-line bg-main-900 p-1 ${
          collapsed ? "m-2.5" : "m-3"
        }`}
      >
        {collapsed || !themePicker ? null : (
          <div
            role="radiogroup"
            aria-label="Тема оформления"
            className="grid grid-cols-3 gap-0.5 rounded-control bg-main-950 p-0.5"
          >
            {THEME_OPTIONS.map((option) => (
              <button
                key={option.value}
                type="button"
                role="radio"
                aria-checked={theme === option.value}
                aria-label={option.label}
                title={option.label}
                onClick={() => onThemeChange(option.value)}
                className={`flex h-7 cursor-pointer items-center justify-center rounded-md anim-on:transition-colors ${
                  theme === option.value
                    ? "bg-main-800 text-accent-medium"
                    : "text-main-400 hover:bg-main-900 hover:text-main-100"
                }`}
              >
                <Icon path={option.icon} size={15} />
              </button>
            ))}
          </div>
        )}

        <Dropdown className="w-full" menuWidth={collapsed ? 160 : 220} menuPlacement="top-left">
          <Dropdown.Trigger
            icon={false}
            rounded=""
            aria-label={identity.name}
            className="h-9 cursor-pointer justify-center gap-0 rounded-control border-0 bg-transparent p-0 hover:bg-main-800 aria-expanded:bg-main-800 anim-on:transition-colors"
          >
            <span className="flex size-7 items-center justify-center rounded-full bg-accent-soft text-xs font-semibold text-accent-medium">
              {identity.initials}
            </span>
          </Dropdown.Trigger>
          <ActionMenu label={identity.name}>
            <ActionMenuItem icon={mdiAccountCircleOutline} onClick={onOpenProfile}>
              Профиль
            </ActionMenuItem>
          </ActionMenu>
        </Dropdown>
      </div>
    </motion.aside>
  );
}
