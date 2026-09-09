import type { ReactNode } from "react";

export type ChipTone = "neutral" | "raised" | "accent" | "selected";

const TONES: Record<ChipTone, string> = {
  neutral: "bg-main-700 text-main-300",
  raised: "bg-main-600 text-accent-light",
  accent: "bg-main-700 text-accent-medium",
  selected: "bg-accent-dark text-main-900",
};

export interface ChipProps {
  readonly tone?: ChipTone;
  readonly title?: string;
  readonly onClick?: () => void;
  readonly children: ReactNode;
}

const BASE =
  "inline-flex h-[20px] flex-none items-center gap-[5px] rounded-[5px] px-[7px] text-[10.5px] font-medium";

export default function Chip({ tone = "neutral", title, onClick, children }: ChipProps) {
  const className = `${BASE} ${TONES[tone]}`;
  if (onClick === undefined) {
    return (
      <span className={className} title={title}>
        {children}
      </span>
    );
  }
  return (
    <button type="button" className={className} title={title} onClick={onClick}>
      {children}
    </button>
  );
}
