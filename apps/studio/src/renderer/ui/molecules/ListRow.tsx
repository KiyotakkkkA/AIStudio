import type { ReactNode } from "react";

export interface ListRowProps {
  readonly leading?: ReactNode;
  readonly trailing?: ReactNode;
  readonly disabled?: boolean;
  readonly selected?: boolean;
  readonly dashed?: boolean;
  readonly onClick?: () => void;
  readonly children: ReactNode;
}

export default function ListRow({
  leading,
  trailing,
  disabled = false,
  selected = false,
  dashed = false,
  onClick,
  children,
}: ListRowProps) {
  const tone = dashed
    ? "border-dashed border-main-750 bg-transparent hover:border-main-500"
    : selected
      ? "border-accent-dark bg-main-750"
      : "border-main-750 bg-main-800 hover:border-main-600";

  return (
    <button
      type="button"
      disabled={disabled}
      aria-current={selected ? "true" : undefined}
      onClick={onClick}
      className={`flex w-full gap-2.75 rounded-card border p-3 text-left ${tone} disabled:cursor-not-allowed disabled:opacity-50`}
    >
      {leading}
      <div className="flex min-w-0 flex-1 flex-col gap-1.25">{children}</div>
      {trailing}
    </button>
  );
}
