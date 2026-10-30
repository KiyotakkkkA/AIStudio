import { useCallback, useEffect, useRef, useState, type MouseEvent, type ReactNode } from "react";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import Icon from "../atoms/Icon";

export const MENU_WIDTH = 192;
const MENU_SURFACE = "rounded-card border-line bg-main-900 p-1";
export const MENU_PANEL = `${MENU_SURFACE} shadow-pop`;
export const MENU_ITEM = "text-[13px] text-main-100";
export const MENU_DANGER = "text-[13px] text-err hover:bg-err-soft hover:text-err";

const SUBMENU_CLOSE_DELAY_MS = 140;
const GEOMETRY = "min-h-7 px-2 py-1 leading-5 disabled:pointer-events-none disabled:opacity-40";

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
      rounded=""
      className={`overflow-visible! ${MENU_PANEL}`}
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
  onClick,
}: {
  readonly icon: string;
  readonly children: ReactNode;
  readonly danger?: boolean;
  readonly disabled?: boolean;
  readonly title?: string;
  readonly onClick: (event: MouseEvent<HTMLButtonElement>) => void;
}) {
  return (
    <Dropdown.Item
      role="menuitem"
      rounded="rounded-md"
      className={`${danger ? MENU_DANGER : MENU_ITEM} ${GEOMETRY}`}
      disabled={disabled}
      title={title}
      onClick={onClick}
    >
      <span className="flex min-w-0 items-center gap-2">
        <Icon path={icon} size={16} className={`flex-none ${danger ? "" : "text-main-400"}`} />
        <span className="min-w-0 flex-1 truncate">{children}</span>
      </span>
    </Dropdown.Item>
  );
}

function Submenu({
  icon,
  label,
  disabled,
  children,
}: {
  readonly icon: string;
  readonly label: string;
  readonly disabled: boolean;
  readonly children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const closing = useRef<ReturnType<typeof setTimeout> | null>(null);
  const cancel = useCallback(() => {
    if (closing.current === null) return;
    clearTimeout(closing.current);
    closing.current = null;
  }, []);
  const show = useCallback(() => {
    cancel();
    if (!disabled) setOpen(true);
  }, [cancel, disabled]);
  const hide = useCallback(() => {
    cancel();
    closing.current = setTimeout(() => setOpen(false), SUBMENU_CLOSE_DELAY_MS);
  }, [cancel]);
  useEffect(() => cancel, [cancel]);
  const expanded = open && !disabled;
  return (
    <div
      className="relative"
      onPointerEnter={show}
      onPointerLeave={hide}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) hide();
      }}
    >
      <Dropdown.Item
        role="menuitem"
        aria-haspopup="menu"
        aria-expanded={expanded}
        rounded="rounded-md"
        className={`${MENU_ITEM} ${GEOMETRY} ${expanded ? "bg-main-700/70 text-main-50" : ""}`}
        disabled={disabled}
        closeOnClick={false}
        onFocus={show}
        onClick={(event) => {
          event.stopPropagation();
          cancel();
          setOpen(!expanded);
        }}
      >
        <span className="flex min-w-0 items-center gap-2">
          <Icon path={icon} size={16} className="flex-none text-main-400" />
          <span className="min-w-0 flex-1 truncate">{label}</span>
          <span
            aria-hidden="true"
            className={`flex-none text-base leading-none anim-on:transition-transform ${
              expanded ? "translate-x-0.5 text-main-300" : "text-main-500"
            }`}
          >
            ›
          </span>
        </span>
      </Dropdown.Item>
      {expanded ? (
        <>
          <div aria-hidden="true" className="absolute top-0 left-full z-40 h-full w-1" />
          <div
            role="menu"
            aria-label={label}
            className={`absolute top-0 left-[calc(100%+4px)] z-50 flex w-48 flex-col gap-0.5 border ${MENU_SURFACE}`}
          >
            {children}
          </div>
        </>
      ) : null}
    </div>
  );
}

export function ActionSubmenu({
  icon,
  label,
  disabled = false,
  children,
}: {
  readonly icon: string;
  readonly label: string;
  readonly disabled?: boolean;
  readonly children: ReactNode;
}) {
  return (
    <Dropdown.Render>
      {({ open }) =>
        open ? (
          <Submenu icon={icon} label={label} disabled={disabled}>
            {children}
          </Submenu>
        ) : null
      }
    </Dropdown.Render>
  );
}

export function ActionMenuSeparator() {
  return <div role="separator" className="mx-2 my-1 h-px bg-line" />;
}
