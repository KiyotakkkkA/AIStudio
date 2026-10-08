import {
  mdiCogOutline,
  mdiDockLeft,
  mdiLightningBolt,
  mdiMonitor,
  mdiWeatherNight,
  mdiWhiteBalanceSunny,
} from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Icon from "../atoms/Icon";
import NavRailItem from "../molecules/NavRailItem";
import type { NavGroupModel, NavItemModel, RailIdentity, RailTheme } from "./NavRailTypes";

const THEME_OPTIONS: readonly { value: RailTheme; label: string; icon: string }[] = [
  { value: "light", label: "Светлая тема", icon: mdiWhiteBalanceSunny },
  { value: "dark", label: "Тёмная тема", icon: mdiWeatherNight },
  { value: "system", label: "Как в системе", icon: mdiMonitor },
];

export interface NavRailProps {
  readonly groups: readonly NavGroupModel[];
  readonly identity: RailIdentity;
  readonly version: string;
  readonly collapsed: boolean;
  readonly theme: RailTheme;
  readonly isActive: (item: NavItemModel) => boolean;
  readonly onSelect: (item: NavItemModel) => void;
  readonly onToggleCollapse: () => void;
  readonly onOpenIdentity: () => void;
  readonly onThemeChange: (theme: RailTheme) => void;
}

const RAIL_ICON_BUTTON =
  "flex size-7 flex-none items-center justify-center rounded-control text-main-500 hover:bg-main-900/70 hover:text-main-100";

export default function NavRail({
  groups,
  identity,
  version,
  collapsed,
  theme,
  isActive,
  onSelect,
  onToggleCollapse,
  onOpenIdentity,
  onThemeChange,
}: NavRailProps) {
  return (
    <aside
      className={`flex h-full flex-none flex-col bg-main-950 ${
        collapsed ? "w-rail-collapsed" : "w-rail"
      }`}
    >
      <div
        className={`flex h-16 flex-none items-center gap-2.5 ${
          collapsed ? "justify-center px-0" : "pr-2 pl-4.5"
        }`}
      >
        <div className="flex size-8 flex-none items-center justify-center rounded-control bg-linear-to-br from-accent-medium to-accent-dark text-on-accent shadow-[inset_0_1px_0_rgb(255_255_255/0.2),0_2px_6px_rgb(180_80_20/0.25)]">
          <Icon path={mdiLightningBolt} size={17} />
        </div>
        {collapsed ? null : (
          <>
            <div className="flex min-w-0 flex-1 flex-col leading-tight">
              <span className="truncate text-[14px] font-semibold tracking-[-0.01em] text-main-50">
                ZVS AI Studio
              </span>
              <span className="font-mono text-[10.5px] text-main-500">{version}</span>
            </div>
            <button
              type="button"
              aria-label="Свернуть панель навигации"
              title="Свернуть панель навигации"
              onClick={onToggleCollapse}
              className={RAIL_ICON_BUTTON}
            >
              <Icon path={mdiDockLeft} size={16} />
            </button>
          </>
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

      <div className={`flex flex-none flex-col gap-2 ${collapsed ? "items-center p-2.5" : "p-3"}`}>
        {collapsed ? null : (
          <div
            role="radiogroup"
            aria-label="Тема оформления"
            className="flex gap-0.5 rounded-control border border-main-750 bg-main-900/60 p-0.75"
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
                className={`flex h-6.5 flex-1 items-center justify-center rounded-[6px] ${
                  theme === option.value
                    ? "bg-main-900 text-accent-medium shadow-[0_1px_2px_rgb(40_25_10/0.1)]"
                    : "text-main-500 hover:text-main-200"
                }`}
              >
                <Icon path={option.icon} size={14} />
              </button>
            ))}
          </div>
        )}

        <div
          className={`flex items-center gap-2.5 ${
            collapsed
              ? "justify-center"
              : "rounded-card border border-main-750 bg-main-900/60 py-2 pr-1.5 pl-2"
          }`}
        >
          <div className="flex size-8 flex-none items-center justify-center rounded-full bg-accent-soft text-[11.5px] font-semibold text-accent-medium ring-1 ring-accent-line">
            {identity.initials}
          </div>
          {collapsed ? null : (
            <>
              <div className="flex min-w-0 flex-1 flex-col leading-tight">
                <span className="truncate text-[13px] font-medium text-main-100">
                  {identity.name}
                </span>
                <span className="truncate text-[11px] text-main-500">{identity.hint}</span>
              </div>
              <button
                type="button"
                aria-label="Открыть настройки"
                title="Открыть настройки"
                onClick={onOpenIdentity}
                className={RAIL_ICON_BUTTON}
              >
                <Icon path={mdiCogOutline} size={16} />
              </button>
            </>
          )}
        </div>

        {collapsed ? (
          <button
            type="button"
            aria-label="Развернуть панель навигации"
            title="Развернуть панель навигации"
            onClick={onToggleCollapse}
            className={RAIL_ICON_BUTTON}
          >
            <Icon path={mdiDockLeft} size={16} className="rotate-180" />
          </button>
        ) : null}
      </div>
    </aside>
  );
}
