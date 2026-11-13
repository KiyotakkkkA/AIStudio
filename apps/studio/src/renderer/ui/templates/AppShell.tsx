import type { ReactNode } from "react";

export interface AppShellProps {
  readonly header: ReactNode;
  readonly rail: ReactNode;
  readonly children: ReactNode;
}

export default function AppShell({ header, rail, children }: AppShellProps) {
  return (
    <div className="flex size-full min-h-0 flex-col overflow-hidden bg-main-950">
      {header}
      <div className="flex min-h-0 flex-1">
        {rail}
        <main className="mr-2 mb-2 flex min-w-0 flex-1 flex-col overflow-hidden rounded-frame bg-main-900 shadow-frame">
          {children}
        </main>
      </div>
    </div>
  );
}
