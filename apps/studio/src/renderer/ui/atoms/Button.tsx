import type { ButtonHTMLAttributes, ReactNode } from "react";

export type ButtonTone = "primary" | "secondary" | "ghost" | "danger";

const TONES: Record<ButtonTone, string> = {
  primary: "bg-accent-dark text-main-900 hover:bg-accent-medium",
  secondary: "border border-main-600 bg-main-700 text-main-100 hover:border-main-500",
  ghost: "border border-main-600 bg-transparent text-main-300 hover:text-main-100",
  danger: "border border-err-border bg-transparent text-err hover:border-err",
};

export type ButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "className"> & {
  readonly tone?: ButtonTone;
  readonly children: ReactNode;
};

export default function Button({ tone = "secondary", children, ...props }: ButtonProps) {
  return (
    <button
      {...props}
      className={`inline-flex h-[32px] flex-none items-center justify-center gap-[7px] rounded-[6px] px-[13px] text-[12.5px] font-semibold disabled:cursor-not-allowed disabled:opacity-50 ${TONES[tone]}`}
    >
      {children}
    </button>
  );
}
