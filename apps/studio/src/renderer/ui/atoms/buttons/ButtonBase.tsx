import type { ButtonHTMLAttributes, ReactNode } from "react";

export type ButtonTone = "primary" | "secondary" | "ghost" | "danger" | "danger-soft";

const TONES: Record<ButtonTone, string> = {
  primary:
    "border-transparent bg-accent-dark text-on-accent shadow-[inset_0_1px_0_rgb(255_255_255/0.14)] hover:bg-accent-medium",
  secondary:
    "border-line bg-main-900 text-main-100 shadow-[0_1px_2px_rgb(40_25_10/0.05)] hover:border-line-strong hover:bg-main-850",
  ghost: "border-transparent bg-transparent text-main-300 hover:bg-main-700 hover:text-main-50",
  danger: "border-transparent bg-danger-medium text-white hover:bg-danger-dark",
  "danger-soft":
    "border-transparent bg-err-soft text-err hover:bg-danger-medium hover:text-white disabled:hover:bg-err-soft disabled:hover:text-err",
};

export type ButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "className"> & {
  readonly tone?: ButtonTone;
  readonly children: ReactNode;
};

export default function ButtonBase({ tone = "secondary", children, ...props }: ButtonProps) {
  return (
    <button
      {...props}
      className={`zvs-press inline-flex h-8 flex-none items-center justify-center gap-1.5 rounded-control border px-3 text-[13px] font-medium whitespace-nowrap select-none disabled:cursor-not-allowed disabled:opacity-45 ${TONES[tone]}`}
    >
      {children}
    </button>
  );
}
