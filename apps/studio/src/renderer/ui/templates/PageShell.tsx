import type { ReactNode } from "react";

export interface PageShellProps {
  readonly title: string;
  readonly subtitle?: string;
  readonly actions?: ReactNode;
  readonly toolbar?: ReactNode;
  readonly children: ReactNode;
}

export default function PageShell({ title, subtitle, actions, toolbar, children }: PageShellProps) {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="flex h-[56px] flex-none items-center gap-3 border-b border-main-700 bg-main-900 px-5">
        <h1 className="flex-none text-[15px] font-semibold text-main-50">{title}</h1>
        {subtitle ? <span className="truncate text-[12px] text-main-500">{subtitle}</span> : null}
        <div className="flex-1" />
        {actions}
      </header>
      {toolbar ? (
        <div className="flex h-11.5 flex-none items-center gap-1.5 border-b border-main-700 bg-main-900 px-5">
          {toolbar}
        </div>
      ) : null}
      <div className="flex min-h-0 flex-1 flex-col gap-4.5 overflow-auto px-5 py-4.5">
        {children}
      </div>
    </div>
  );
}
