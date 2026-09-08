import { mdiCogOutline, mdiDockLeft, mdiLightningBolt } from "@mdi/js";
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
      className={`flex h-full flex-none flex-col border-r border-main-700 bg-main-900 ${
        collapsed ? "w-[64px]" : "w-[248px]"
      }`}
    >
      <div
        className={`flex h-[56px] flex-none items-center gap-[10px] border-b border-main-800 ${
          collapsed ? "justify-center px-0" : "px-[16px]"
        }`}
      >
        <div className="flex size-[28px] flex-none items-center justify-center rounded-[8px] bg-accent-dark text-main-900">
          <Icon path={mdiLightningBolt} size={15} />
        </div>
        {collapsed ? null : (
          <>
            <div className="flex min-w-0 flex-1 flex-col leading-[1.25]">
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
              className="flex size-[24px] flex-none items-center justify-center rounded-[6px] text-main-500 hover:bg-main-800 hover:text-main-200"
            >
              <Icon path={mdiDockLeft} size={16} />
            </button>
          </>
        )}
      </div>

      <nav className="flex min-h-0 flex-1 flex-col gap-[2px] overflow-y-auto px-[10px] py-[12px]">
        {groups.map((group, index) => (
          <div key={group.id} className="flex flex-col gap-[2px]">
            {collapsed ? (
              index === 0 ? null : (
                <div className="my-[7px] h-px bg-main-800" />
              )
            ) : (
              <div
                className={`px-[8px] pb-[6px] text-[10px] font-medium tracking-[0.09em] text-main-500 uppercase ${
                  index === 0 ? "" : "mt-[14px]"
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

      <div
        className={`flex flex-none items-center gap-[10px] border-t border-main-800 p-[10px] ${
          collapsed ? "justify-center" : ""
        }`}
      >
        <div className="flex size-[28px] flex-none items-center justify-center rounded-full bg-main-600 text-[11px] font-semibold text-accent-light">
          {identity.initials}
        </div>
        {collapsed ? null : (
          <>
            <div className="flex min-w-0 flex-1 flex-col leading-[1.25]">
              <span className="truncate text-[12.5px] font-medium">{identity.name}</span>
              <span className="truncate text-[10.5px] text-main-500">{identity.hint}</span>
            </div>
            <button
              type="button"
              aria-label="Открыть настройки"
              title="Открыть настройки"
              onClick={onOpenIdentity}
              className="flex size-[24px] flex-none items-center justify-center rounded-[6px] text-main-500 hover:bg-main-800 hover:text-main-200"
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
          className="flex h-[32px] flex-none items-center justify-center border-t border-main-800 text-main-500 hover:bg-main-800 hover:text-main-200"
        >
          <Icon path={mdiDockLeft} size={16} className="rotate-180" />
        </button>
      ) : null}
    </aside>
  );
}
