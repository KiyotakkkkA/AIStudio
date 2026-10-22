import type { ReactNode } from "react";
import Icon from "../atoms/Icon";

export interface InfoRow {
  readonly key: string;
  readonly icon: string;
  readonly label: string;
  readonly value: ReactNode;
  readonly hint?: string;
  readonly mono?: boolean;
  readonly muted?: boolean;
}

export interface InfoTableProps {
  readonly rows: readonly InfoRow[];
  readonly label?: string;
}

export default function InfoTable({ rows, label }: InfoTableProps) {
  return (
    <dl
      aria-label={label}
      className="m-0 flex flex-col divide-y divide-line overflow-hidden rounded-card border border-line bg-main-850"
    >
      {rows.map((row) => (
        <div key={row.key} className="flex items-center gap-3 px-3.5 py-2.75">
          <span className="flex size-7.5 flex-none items-center justify-center rounded-control bg-main-700 text-accent-medium">
            <Icon path={row.icon} size={15} />
          </span>
          <dt className="w-44 flex-none text-[12px] text-main-400">{row.label}</dt>
          <dd className="m-0 flex min-w-0 flex-1 flex-col">
            <span
              className={`min-w-0 truncate text-[12.5px] ${row.muted === true ? "text-main-500" : "text-main-100"} ${row.mono === true ? "font-mono" : ""}`}
              title={typeof row.value === "string" ? row.value : undefined}
            >
              {row.value}
            </span>
            {row.hint === undefined ? null : (
              <span className="truncate text-[11px] text-main-500">{row.hint}</span>
            )}
          </dd>
        </div>
      ))}
    </dl>
  );
}
