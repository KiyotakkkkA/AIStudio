import type { ReactNode } from "react";

export interface ListRowProps {
  readonly leading?: ReactNode;
  readonly trailing?: ReactNode;
  readonly selected?: boolean;
  readonly dashed?: boolean;
  readonly onClick?: () => void;
  readonly children: ReactNode;
}

export default function ListRow({
  leading,
  trailing,
  selected = false,
  dashed = false,
  onClick,
  children,
}: ListRowProps) {
  const tone = dashed
    ? "border-dashed border-main-600 bg-transparent hover:border-main-500"
    : selected
      ? "border-accent-dark bg-main-750"
      : "border-main-700 bg-main-800 hover:border-main-600";

  return (
    <button
      type="button"
      aria-current={selected ? "true" : undefined}
      onClick={onClick}
      className={`flex w-full gap-[11px] rounded-[10px] border p-[12px] text-left ${tone}`}
    >
      {leading}
      <div className="flex min-w-0 flex-1 flex-col gap-[5px]">{children}</div>
      {trailing}
    </button>
  );
}
