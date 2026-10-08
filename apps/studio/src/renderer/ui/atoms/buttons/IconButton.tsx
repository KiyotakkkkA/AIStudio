import type { ButtonHTMLAttributes } from "react";
import Icon from "../Icon";

export type IconButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> & {
  readonly path: string;
  readonly iconSize?: number;
};

export default function IconButton({
  path,
  iconSize = 15,
  className = "",
  ...props
}: IconButtonProps) {
  return (
    <button
      type="button"
      className={`flex size-7 flex-none items-center justify-center rounded-control text-main-400 hover:bg-main-700 hover:text-main-50 disabled:pointer-events-none disabled:opacity-40 ${className}`}
      {...props}
    >
      <Icon path={path} size={iconSize} />
    </button>
  );
}
