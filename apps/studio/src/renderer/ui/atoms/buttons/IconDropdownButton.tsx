import type { ComponentProps } from "react";
import { Dropdown, Tooltip } from "@kiyotakkkka/zvs-uikit-lib";
import Icon from "../Icon";

export type IconDropdownButtonProps = Omit<
  ComponentProps<typeof Dropdown.Trigger>,
  "children" | "icon"
> & {
  readonly label: string;
  readonly path: string;
  readonly iconSize?: number;
};

export default function IconDropdownButton({
  label,
  path,
  iconSize = 20,
  ...props
}: IconDropdownButtonProps) {
  return (
    <Tooltip label={label} placement="top-center" rounded="rounded-lg" className="">
      <Dropdown.Trigger {...props} icon={<Icon path={path} size={iconSize} />}>
        <span className="sr-only">{label}</span>
      </Dropdown.Trigger>
    </Tooltip>
  );
}
