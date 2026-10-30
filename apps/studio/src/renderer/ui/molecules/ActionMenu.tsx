import type { MouseEvent, ReactNode } from "react";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import Icon from "../atoms/Icon";

const ITEM = "text-[13px] text-main-100 disabled:opacity-45";
const DANGER = "text-[13px] text-err hover:bg-err-soft hover:text-err disabled:opacity-45";

export function ActionMenu({
  label,
  children,
}: {
  readonly label: string;
  readonly children: ReactNode;
}) {
  return (
    <Dropdown.Menu
      role="menu"
      aria-label={label}
      rounded="rounded-lg"
      className="rounded-card border-line bg-main-900 p-1 shadow-pop"
    >
      {children}
    </Dropdown.Menu>
  );
}

export function ActionMenuItem({
  icon,
  children,
  danger = false,
  disabled = false,
  title,
  closeOnClick = true,
  inset = false,
  expanded,
  trailing,
  onClick,
}: {
  readonly icon: string;
  readonly children: ReactNode;
  readonly danger?: boolean;
  readonly disabled?: boolean;
  readonly title?: string;
  readonly closeOnClick?: boolean;
  readonly inset?: boolean;
  readonly expanded?: boolean;
  readonly trailing?: ReactNode;
  readonly onClick: (event: MouseEvent<HTMLButtonElement>) => void;
}) {
  return (
    <Dropdown.Item
      role="menuitem"
      rounded="rounded-lg"
      className={danger ? DANGER : ITEM}
      disabled={disabled}
      title={title}
      closeOnClick={closeOnClick}
      aria-expanded={expanded}
      onClick={onClick}
    >
      <span className={`flex min-w-0 items-center gap-2 ${inset ? "pl-6" : ""}`}>
        <Icon path={icon} size={16} className={`flex-none ${danger ? "" : "text-main-400"}`} />
        <span className="min-w-0 flex-1 truncate">{children}</span>
        {trailing}
      </span>
    </Dropdown.Item>
  );
}

export function ActionMenuSeparator() {
  return <div role="separator" className="mx-2 my-1 h-px bg-line" />;
}
