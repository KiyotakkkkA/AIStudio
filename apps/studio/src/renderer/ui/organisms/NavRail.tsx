import {
  mdiAccountCircleOutline,
  mdiMonitor,
  mdiWeatherNight,
  mdiWhiteBalanceSunny,
} from "@mdi/js";
import { Dropdown, ScrollArea, Switcher } from "@kiyotakkkka/zvs-uikit-lib";
import { motion } from "motion/react";
import AppLogo from "../atoms/AppLogo";
import Icon from "../atoms/Icon";
import ProfileAvatar from "../atoms/ProfileAvatar";
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
  readonly avatarUrl: string | null;
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
  avatarUrl,
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
            AI Studio Desktop
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

      <div className={`flex flex-none items-center gap-2 ${collapsed ? "p-2.5" : "p-3"}`}>
        <Dropdown
          className="w-full flex-1"
          menuWidth={collapsed ? 160 : 220}
          menuPlacement="top-left"
        >
          <Dropdown.Trigger
            icon={false}
            rounded=""
            aria-label={identity.name}
            className={`h-9 cursor-pointer w-fit gap-0 rounded-control border-0 bg-transparent py-0 hover:bg-main-850 aria-expanded:bg-main-850 anim-on:transition-colors ${
              collapsed ? "justify-center px-0" : "justify-start px-1"
            }`}
          >
            <ProfileAvatar
              initials={identity.initials}
              url={avatarUrl}
              className="size-7 text-xs"
            />
          </Dropdown.Trigger>
          <ActionMenu label={identity.name}>
            <ActionMenuItem icon={mdiAccountCircleOutline} onClick={onOpenProfile}>
              Профиль
            </ActionMenuItem>
          </ActionMenu>
        </Dropdown>

        {collapsed || !themePicker ? null : (
          <Switcher.Root
            model={theme}
            onModelChange={(value) => {
              const option = THEME_OPTIONS.find((entry) => entry.value === value);
              if (option) onThemeChange(option.value);
            }}
            label="Тема оформления"
            rounded=""
            className="shrink-0 gap-0.5 rounded-control border-line bg-main-900 p-0.75"
          >
            {THEME_OPTIONS.map((option) => (
              <Switcher.Option
                key={option.value}
                value={option.value}
                aria-label={option.label}
                className={`h-7 w-8 rounded-md p-0 ${
                  theme === option.value
                    ? "bg-main-700 text-main-50"
                    : "text-main-400 enabled:hover:bg-main-850 enabled:hover:text-main-100"
                }`}
              >
                <Icon path={option.icon} size={15} />
              </Switcher.Option>
            ))}
          </Switcher.Root>
        )}
      </div>
    </motion.aside>
  );
}
