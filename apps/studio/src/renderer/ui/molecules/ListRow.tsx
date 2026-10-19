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
    ? "border-dashed border-line-strong bg-transparent text-main-400 hover:border-accent-line hover:bg-accent-soft hover:text-accent-medium"
    : selected
      ? "border-transparent bg-accent-soft"
      : "border-transparent bg-transparent hover:bg-main-800";

  return (
    <button
      type="button"
      disabled={disabled}
      aria-current={selected ? "true" : undefined}
      onClick={onClick}
      className={`flex w-full gap-3 rounded-card border px-3.5 py-3 text-left ${tone} disabled:cursor-not-allowed disabled:opacity-50`}
    >
      {leading}
      <div className="flex min-w-0 flex-1 flex-col gap-1">{children}</div>
      {trailing}
    </button>
  );
}
