import StatusDot from "../atoms/StatusDot";
import type { StatusTone } from "../atoms/statusTone";

export interface FilterRailButtonProps {
  readonly active: boolean;
  readonly label: string;
  readonly count: number;
  readonly tone?: StatusTone;
  readonly onClick: () => void;
}

export default function FilterRailButton({
  active,
  label,
  count,
  tone,
  onClick,
}: FilterRailButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={`flex h-8 items-center gap-2.5 rounded-control px-2.5 text-[13px] ${
        active
          ? "bg-main-900 font-medium text-main-50 shadow-[0_1px_2px_rgb(40_25_10/0.08)] ring-1 ring-main-750 ring-inset"
          : "text-main-300 hover:bg-main-700 hover:text-main-100"
      }`}
    >
      {tone === undefined ? null : <StatusDot tone={tone} />}
      <span className="min-w-0 flex-1 truncate text-left">{label}</span>
      <span
        className={`flex-none rounded-full px-1.5 text-[11px] font-medium tabular-nums ${
          active ? "bg-accent-soft text-accent-medium" : "text-main-500"
        }`}
      >
        {count}
      </span>
    </button>
  );
}
