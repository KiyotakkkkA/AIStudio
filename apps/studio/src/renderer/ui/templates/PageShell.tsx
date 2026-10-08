import type { ReactNode } from "react";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import PageHeader from "../organisms/PageHeader";

export interface PageShellProps {
  readonly icon: string;
  readonly title: string;
  readonly subtitle?: string;
  readonly actions?: ReactNode;
  readonly toolbar?: ReactNode;
  readonly children: ReactNode;
}

export default function PageShell({
  icon,
  title,
  subtitle,
  actions,
  toolbar,
  children,
}: PageShellProps) {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader icon={icon} title={title} subtitle={subtitle} actions={actions} />
      {toolbar ? (
        <div className="flex h-12 flex-none items-center gap-2 border-b border-main-750 bg-main-850 px-6">
          {toolbar}
        </div>
      ) : null}
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-5 px-6 py-5">{children}</ScrollArea>
    </div>
  );
}
