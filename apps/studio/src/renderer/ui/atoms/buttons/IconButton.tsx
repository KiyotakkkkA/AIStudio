import type { ButtonHTMLAttributes, ComponentProps } from "react";
import { Tooltip } from "@kiyotakkkka/zvs-uikit-lib";
import Icon from "../Icon";
import type { ButtonSize } from "./ButtonBase";
import useConfirmedClick, { type ConfirmProps } from "./useConfirmedClick";

export type TooltipPlacement = NonNullable<ComponentProps<typeof Tooltip>["placement"]>;

export type IconButtonTone = "ghost" | "primary" | "danger";

export type IconButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children" | "title"> &
  ConfirmProps & {
    readonly path: string;
    readonly label: string;
    readonly tooltip?: TooltipPlacement;
    readonly iconSize?: number;
    readonly tone?: IconButtonTone;
    readonly size?: ButtonSize;
    readonly toneClassName?: string;
  };

const TONES: Record<IconButtonTone, string> = {
  ghost: "text-main-400 hover:bg-main-700 hover:text-main-50",
  primary:
    "zvs-press border border-transparent bg-accent-dark text-on-accent shadow-[0_1px_2px_rgb(40_25_10/0.12)] hover:bg-accent-medium",
  danger: "zvs-press border border-transparent bg-danger-medium text-white hover:bg-danger-dark",
};

const SIZES: Record<ButtonSize, string> = {
  sm: "size-7",
  md: "size-8",
};

export default function IconButton({
  path,
  label,
  tooltip = "top-center",
  iconSize = 15,
  tone = "ghost",
  size = "sm",
  toneClassName = TONES[tone],
  className = "",
  needConfirm,
  modalSetup,
  disabled,
  onClick,
  "aria-label": ariaLabel,
  ...props
}: IconButtonProps) {
  const confirm = useConfirmedClick({ needConfirm, modalSetup, disabled, onClick });

  return (
    <>
      <Tooltip label={label} placement={tooltip} className="">
        <button
          type="button"
          aria-label={ariaLabel ?? label}
          className={`flex ${SIZES[size]} flex-none items-center justify-center rounded-control ${toneClassName} disabled:pointer-events-none disabled:opacity-40 ${className}`}
          {...props}
          disabled={disabled}
          onClick={confirm.onClick}
        >
          <Icon path={path} size={iconSize} />
        </button>
      </Tooltip>
      {confirm.modal}
    </>
  );
}
