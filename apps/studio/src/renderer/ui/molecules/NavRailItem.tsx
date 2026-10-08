import { mdiOpenInNew } from "@mdi/js";
import Icon from "../atoms/Icon";
import type { NavItemModel, NavStatus } from "../organisms/NavRailTypes";

const STATUS_COLOR: Record<NavStatus, string> = {
  accent: "bg-accent-dark",
  ok: "bg-ok",
  warn: "bg-warn",
  err: "bg-err",
};

export interface NavRailItemProps {
  readonly item: NavItemModel;
  readonly active: boolean;
  readonly collapsed: boolean;
  readonly onSelect: (item: NavItemModel) => void;
}

export default function NavRailItem({ item, active, collapsed, onSelect }: NavRailItemProps) {
  return (
    <button
      type="button"
      title={collapsed ? item.label : undefined}
      aria-current={active ? "page" : undefined}
      onClick={() => onSelect(item)}
      className={[
        "group flex h-8.5 w-full flex-none items-center gap-2.5 rounded-control text-[13px]",
        collapsed ? "justify-center px-0" : "px-2.5",
        active
          ? "bg-main-100 font-medium text-main-900 shadow-[0_1px_2px_rgb(30_20_10/0.18)]"
          : "text-main-300 hover:bg-main-900/70 hover:text-main-50",
      ].join(" ")}
    >
      <span className="relative flex flex-none items-center justify-center">
        <Icon
          path={item.icon}
          size={17}
          className={active ? "text-main-900" : "text-main-400 group-hover:text-main-200"}
        />
        {collapsed && item.status ? (
          <span
            className={`absolute -top-0.5 -right-1 size-2 flex-none rounded-full ring-2 ring-main-950 ${STATUS_COLOR[item.status]}`}
          />
        ) : null}
      </span>
      {collapsed ? null : (
        <>
          <span className="flex-1 truncate text-left">{item.label}</span>
          {item.badge === undefined ? null : (
            <span
              className={`inline-flex h-4.5 min-w-4.5 flex-none items-center justify-center rounded-full px-1.5 text-[10.5px] font-semibold tabular-nums ${
                active ? "bg-main-900/20 text-main-900" : "bg-main-700 text-main-300"
              }`}
            >
              {item.badge}
            </span>
          )}
          {item.status ? (
            <span className={`size-2 flex-none rounded-full ${STATUS_COLOR[item.status]}`} />
          ) : null}
          {item.external ? (
            <Icon path={mdiOpenInNew} size={12} className="flex-none opacity-55" />
          ) : null}
        </>
      )}
    </button>
  );
}
