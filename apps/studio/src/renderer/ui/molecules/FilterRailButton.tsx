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
      className={`flex h-7.5 items-center gap-2.25 rounded-lg px-2.25 text-[12.5px] ${
        active ? "bg-main-750 text-main-50" : "text-main-300 hover:bg-main-800"
      }`}
    >
      {tone === undefined ? null : <StatusDot tone={tone} />}
      <span className="min-w-0 flex-1 truncate text-left">{label}</span>
      <span className="flex-none font-mono text-[11px] text-main-500">{count}</span>
    </button>
  );
}
