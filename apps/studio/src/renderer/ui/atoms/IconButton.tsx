import type { ButtonHTMLAttributes } from "react";
import Icon from "./Icon";

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
      className={`flex size-7 items-center justify-center rounded-md text-main-400 hover:bg-main-700 hover:text-main-100 ${className}`}
      {...props}
    >
      <Icon path={path} size={iconSize} />
    </button>
  );
}
