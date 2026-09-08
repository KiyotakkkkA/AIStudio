import type { ReactNode } from "react";

export interface AppShellProps {
  readonly rail: ReactNode;
  readonly children: ReactNode;
}

export default function AppShell({ rail, children }: AppShellProps) {
  return (
    <div className="flex h-full min-h-0 w-full overflow-hidden bg-main-800">
      {rail}
      <div className="flex min-w-0 flex-1 flex-col">{children}</div>
    </div>
  );
}
