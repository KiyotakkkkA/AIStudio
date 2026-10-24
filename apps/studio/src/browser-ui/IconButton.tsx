import type { ButtonHTMLAttributes, ComponentProps } from "react";
import { Tooltip } from "@kiyotakkkka/zvs-uikit-lib";
import Icon from "./Icon";

export type TooltipPlacement = NonNullable<ComponentProps<typeof Tooltip>["placement"]>;

export type IconButtonProps = Omit<
  ButtonHTMLAttributes<HTMLButtonElement>,
  "children" | "title"
> & {
  readonly path: string;
  readonly label: string;
  readonly tooltip?: TooltipPlacement;
  readonly iconSize?: number;
};

export default function IconButton({
  path,
  label,
  tooltip = "top-center",
  iconSize = 16,
  className = "",
  "aria-label": ariaLabel,
  ...props
}: IconButtonProps) {
  return (
    <Tooltip label={label} placement={tooltip} rounded="rounded-lg" className="">
      <button
        type="button"
        aria-label={ariaLabel ?? label}
        className={`icon-button ${className}`}
        {...props}
      >
        <Icon path={path} size={iconSize} />
      </button>
    </Tooltip>
  );
}
