import type { ComponentProps } from "react";
import { Dropdown, Tooltip } from "@kiyotakkkka/zvs-uikit-lib";
import Icon from "../Icon";
import type { TooltipPlacement } from "./IconButton";

export type IconDropdownButtonSize = "xs" | "sm" | "md";

export type IconDropdownButtonProps = Omit<
  ComponentProps<typeof Dropdown.Trigger>,
  "children" | "icon"
> & {
  readonly label: string;
  readonly path: string;
  readonly iconSize?: number;
  readonly tooltip?: TooltipPlacement;
  readonly size?: IconDropdownButtonSize;
  readonly toneClassName?: string;
};

const BASE =
  "flex-none justify-center gap-0 border-none p-0 cursor-auto disabled:pointer-events-none disabled:opacity-40";

const TONE = "bg-transparent text-main-400 hover:bg-main-700 hover:text-main-100";

const SIZES: Record<IconDropdownButtonSize, string> = {
  xs: "size-6",
  sm: "size-7",
  md: "size-8",
};

export default function IconDropdownButton({
  label,
  path,
  iconSize = 18,
  tooltip = "top-center",
  size = "md",
  toneClassName = TONE,
  rounded = "rounded-lg",
  className = "",
  ...props
}: IconDropdownButtonProps) {
  return (
    <Tooltip label={label} placement={tooltip} className="">
      <Dropdown.Trigger
        {...props}
        rounded={rounded}
        className={`${BASE} ${SIZES[size]} ${toneClassName} ${className}`}
        icon={<Icon path={path} size={iconSize} />}
      >
        <span className="sr-only">{label}</span>
      </Dropdown.Trigger>
    </Tooltip>
  );
}
