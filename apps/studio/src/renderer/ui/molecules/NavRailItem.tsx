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
        "flex h-[32px] w-full flex-none items-center gap-[10px] rounded-[7px] text-[13px] font-medium",
        collapsed ? "justify-center px-0" : "px-[8px]",
        active
          ? "bg-main-700 text-main-50 shadow-[inset_2px_0_0_var(--color-accent-dark)]"
          : "text-main-300 hover:bg-main-800 hover:text-main-100",
      ].join(" ")}
    >
      <span className="relative flex flex-none items-center justify-center">
        <Icon path={item.icon} />
        {collapsed && item.status ? (
          <span
            className={`absolute -top-px -right-0.5 size-1.5 flex-none rounded-full ${STATUS_COLOR[item.status]}`}
          />
        ) : null}
      </span>
      {collapsed ? null : (
        <>
          <span className="flex-1 truncate text-left">{item.label}</span>
          {item.badge === undefined ? null : (
            <span
              className={`inline-flex h-4.75 flex-none items-center rounded-pill px-1.75 text-[10px] font-medium ${
                active ? "bg-main-600 text-main-200" : "bg-main-800 text-main-300"
              }`}
            >
              {item.badge}
            </span>
          )}
          {item.status ? (
            <span className={`size-1.75 flex-none rounded-full ${STATUS_COLOR[item.status]}`} />
          ) : null}
          {item.external ? (
            <Icon path={mdiOpenInNew} size={12} className="flex-none opacity-55" />
          ) : null}
        </>
      )}
    </button>
  );
}
