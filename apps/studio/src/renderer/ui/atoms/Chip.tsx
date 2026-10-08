import type { ReactNode } from "react";

type ChipTone = "neutral" | "raised" | "accent" | "selected";

const TONES: Record<ChipTone, string> = {
  neutral: "border-main-750 bg-main-800 text-main-300",
  raised: "border-accent-line bg-accent-soft text-accent-medium",
  accent: "border-transparent bg-accent-soft text-accent-medium",
  selected: "border-transparent bg-accent-dark text-on-accent",
};

export interface ChipProps {
  readonly tone?: ChipTone;
  readonly title?: string;
  readonly onClick?: () => void;
  readonly children: ReactNode;
}

const BASE =
  "inline-flex h-5 flex-none items-center gap-1 rounded-full border px-2 text-[11px] leading-none font-medium whitespace-nowrap";

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
