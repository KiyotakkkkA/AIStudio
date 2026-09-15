import { mdiCogOutline, mdiDockLeft, mdiLightningBolt } from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Icon from "../atoms/Icon";
import NavRailItem from "../molecules/NavRailItem";
import type { NavGroupModel, NavItemModel, RailIdentity } from "./NavRailTypes";

export interface NavRailProps {
  readonly groups: readonly NavGroupModel[];
  readonly identity: RailIdentity;
  readonly version: string;
  readonly collapsed: boolean;
  readonly isActive: (item: NavItemModel) => boolean;
  readonly onSelect: (item: NavItemModel) => void;
  readonly onToggleCollapse: () => void;
  readonly onOpenIdentity: () => void;
}

export default function NavRail({
  groups,
  identity,
  version,
  collapsed,
  isActive,
  onSelect,
  onToggleCollapse,
  onOpenIdentity,
}: NavRailProps) {
  return (
    <aside
      className={`flex h-full flex-none flex-col border-r border-main-750 bg-main-900 ${
        collapsed ? "w-[64px]" : "w-rail"
      }`}
    >
      <div
        className={`flex h-[56px] flex-none items-center gap-2.5 border-b border-main-750 ${
          collapsed ? "justify-center px-0" : "px-4"
        }`}
      >
        <div className="flex size-7 flex-none items-center justify-center rounded-lg bg-accent-dark text-main-900">
          <Icon path={mdiLightningBolt} size={15} />
        </div>
        {collapsed ? null : (
          <>
            <div className="flex min-w-0 flex-1 flex-col leading-tight">
              <span className="truncate text-[13px] font-semibold tracking-[0.01em]">
                ZVS AI Studio
              </span>
              <span className="font-mono text-[10px] text-main-500">{version}</span>
            </div>
            <button
              type="button"
              aria-label="Свернуть панель навигации"
              title="Свернуть панель навигации"
              onClick={onToggleCollapse}
              className="flex size-6 flex-none items-center justify-center rounded-[6px] text-main-500 hover:bg-main-800 hover:text-main-200"
            >
              <Icon path={mdiDockLeft} size={16} />
            </button>
          </>
        )}
      </div>

      <ScrollArea className="min-h-0 flex-1 px-2 py-2.5">
        <nav className="flex flex-col gap-0.5">
          {groups.map((group, index) => (
            <div key={group.id} className="flex flex-col gap-0.5">
              {collapsed ? (
                index === 0 ? null : (
                  <div className="my-1.5 h-px bg-main-800" />
                )
              ) : (
                <div
                  className={`px-2 pb-1.5 text-[10px] font-medium tracking-[0.09em] text-main-500 uppercase ${
                    index === 0 ? "" : "mt-3.5"
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
        className={`flex flex-none items-center gap-2.5 border-t border-main-750 p-2.5 ${
          collapsed ? "justify-center" : ""
        }`}
      >
        <div className="flex size-7 flex-none items-center justify-center rounded-full bg-main-600 text-[11px] font-semibold text-accent-light">
          {identity.initials}
        </div>
        {collapsed ? null : (
          <>
            <div className="flex min-w-0 flex-1 flex-col leading-tight">
              <span className="truncate text-[12.5px] font-medium">{identity.name}</span>
              <span className="truncate text-[10.5px] text-main-500">{identity.hint}</span>
            </div>
            <button
              type="button"
              aria-label="Открыть настройки"
              title="Открыть настройки"
              onClick={onOpenIdentity}
              className="flex size-6 flex-none items-center justify-center rounded-[6px] text-main-500 hover:bg-main-800 hover:text-main-200"
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
          className="flex h-8 flex-none items-center justify-center border-t border-main-750 text-main-500 hover:bg-main-800 hover:text-main-200"
        >
          <Icon path={mdiDockLeft} size={16} className="rotate-180" />
        </button>
      ) : null}
    </aside>
  );
}
