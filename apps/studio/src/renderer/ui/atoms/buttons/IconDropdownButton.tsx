import type { ComponentProps } from "react";
import { Dropdown, Tooltip } from "@kiyotakkkka/zvs-uikit-lib";
import Icon from "../Icon";
import type { TooltipPlacement } from "./IconButton";

export type IconDropdownButtonProps = Omit<
  ComponentProps<typeof Dropdown.Trigger>,
  "children" | "icon"
> & {
  readonly label: string;
  readonly path: string;
  readonly iconSize?: number;
  readonly tooltip?: TooltipPlacement;
};

export default function IconDropdownButton({
  label,
  path,
  iconSize = 20,
  tooltip = "top-center",
  ...props
}: IconDropdownButtonProps) {
  return (
    <Tooltip label={label} placement={tooltip} rounded="rounded-lg" className="">
      <Dropdown.Trigger {...props} icon={<Icon path={path} size={iconSize} />}>
        <span className="sr-only">{label}</span>
      </Dropdown.Trigger>
    </Tooltip>
  );
}
