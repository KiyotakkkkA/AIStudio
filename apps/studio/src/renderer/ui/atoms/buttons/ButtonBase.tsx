import type { ButtonHTMLAttributes, ReactNode } from "react";

export type ButtonTone = "primary" | "secondary" | "ghost" | "danger" | "danger-soft";
export type ButtonSize = "sm" | "md";

const TONES: Record<ButtonTone, string> = {
  primary:
    "border-transparent bg-accent-dark text-on-accent shadow-[0_1px_2px_rgb(40_25_10/0.12)] hover:bg-accent-medium",
  secondary:
    "border-line bg-main-900 text-main-100 shadow-[0_1px_2px_rgb(40_25_10/0.05)] hover:border-line-strong hover:bg-main-850",
  ghost: "border-transparent bg-transparent text-main-300 hover:bg-main-700 hover:text-main-50",
  danger: "border-transparent bg-danger-medium text-white hover:bg-danger-dark",
  "danger-soft":
    "border-transparent bg-err-soft text-err hover:bg-danger-medium hover:text-white disabled:hover:bg-err-soft disabled:hover:text-err",
};

const SIZES: Record<ButtonSize, string> = {
  sm: "h-7 gap-1 px-2.5 text-[12.5px] has-[>svg:first-child]:pl-2 [&>svg]:size-[13px]",
  md: "h-8 gap-1.5 px-3 text-[13px] has-[>svg:first-child]:pl-2.5 [&>svg]:size-3.5",
};

export type ButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "className"> & {
  readonly tone?: ButtonTone;
  readonly size?: ButtonSize;
  readonly children: ReactNode;
};

export default function ButtonBase({
  tone = "secondary",
  size = "md",
  type = "button",
  children,
  ...props
}: ButtonProps) {
  return (
    <button
      {...props}
      type={type}
      className={`zvs-press inline-flex flex-none items-center justify-center rounded-control border font-medium whitespace-nowrap select-none disabled:cursor-not-allowed disabled:opacity-45 [&>svg]:flex-none ${SIZES[size]} ${TONES[tone]}`}
    >
      {children}
    </button>
  );
}
