export interface BarPart {
  readonly key: string;
  readonly value: number;
  readonly fill: string;
}

export function share(value: number, total: number): number {
  return total > 0 ? Math.min(100, (value / total) * 100) : 0;
}

export function percent(value: number, total: number): string {
  const ratio = share(value, total);
  if (ratio === 0) return "0%";
  if (ratio < 1) return "<1%";
  return `${String(Math.round(ratio))}%`;
}

export function StackedBar({
  parts,
  total,
  className,
}: {
  readonly parts: readonly BarPart[];
  readonly total: number;
  readonly className: string;
}) {
  return (
    <div className={`flex overflow-hidden rounded-full bg-main-700 ${className}`}>
      {parts.map((part) =>
        part.value > 0 ? (
          <span
            key={part.key}
            className={`h-full min-w-0.5 anim-on:transition-[width] anim-on:duration-300 ${part.fill}`}
            style={{ width: `${String(share(part.value, total))}%` }}
          />
        ) : null,
      )}
    </div>
  );
}

export function MeterBar({
  value,
  total,
  fill,
  className = "w-16",
}: {
  readonly value: number;
  readonly total: number;
  readonly fill: string;
  readonly className?: string;
}) {
  return (
    <span className={`h-1 shrink-0 overflow-hidden rounded-full bg-main-700 ${className}`}>
      <span
        className={`block h-full rounded-full anim-on:transition-[width] anim-on:duration-300 ${fill}`}
        style={{ width: `${String(Math.max(4, share(value, total)))}%` }}
      />
    </span>
  );
}
