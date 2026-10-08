import type { ReactNode } from "react";

export interface AppShellProps {
  readonly rail: ReactNode;
  readonly children: ReactNode;
}

export default function AppShell({ rail, children }: AppShellProps) {
  return (
    <div className="flex size-full min-h-0 overflow-hidden bg-main-950">
      {rail}
      <main className="my-2 mr-2 flex min-w-0 flex-1 flex-col overflow-hidden rounded-frame border border-main-750 bg-main-900 shadow-frame">
        {children}
      </main>
    </div>
  );
}
