import StatusDot from "../../ui/atoms/StatusDot";
import type { StatusTone } from "../../ui/atoms/statusTone";

export interface TaskFilterButtonProps {
  readonly active: boolean;
  readonly label: string;
  readonly count: number;
  readonly tone?: StatusTone;
  readonly onClick: () => void;
}

export default function TaskFilterButton({
  active,
  label,
  count,
  tone,
  onClick,
}: TaskFilterButtonProps) {
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
