import type { ReactNode } from "react";
import Icon from "../atoms/Icon";

export interface PageHeaderProps {
  readonly icon: string;
  readonly title: string;
  readonly subtitle?: ReactNode;
  readonly meta?: ReactNode;
  readonly actions?: ReactNode;
}

export default function PageHeader({ icon, title, subtitle, meta, actions }: PageHeaderProps) {
  return (
    <header className="flex h-header flex-none items-center gap-4 px-6">
      <div className="flex min-w-0 flex-1 items-center gap-3">
        <span className="flex size-9 flex-none items-center justify-center rounded-control bg-accent-soft text-accent-medium">
          <Icon path={icon} size={18} />
        </span>
        <div className="flex min-w-0 flex-col">
          <div className="flex min-w-0 items-center gap-2">
            <h1 className="m-0 truncate text-[17px]/6 font-semibold tracking-[-0.015em] text-main-50">
              {title}
            </h1>
            {meta}
          </div>
          {subtitle ? (
            <span
              className="truncate text-[12px]/4 text-main-500"
              title={typeof subtitle === "string" ? subtitle : undefined}
            >
              {subtitle}
            </span>
          ) : null}
        </div>
      </div>
      {actions ? <div className="flex flex-none items-center gap-2">{actions}</div> : null}
    </header>
  );
}
