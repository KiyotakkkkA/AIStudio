import type { ReactNode } from "react";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Icon from "../atoms/Icon";

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
      <header className="flex h-[56px] flex-none items-center gap-3 border-b border-main-750 bg-main-900 px-5">
        <div className="flex min-w-0 flex-1 items-center gap-2.5">
          <Icon path={icon} size={20} className="flex-none text-accent-dark" />
          <h1 className="m-0 flex-none text-xl/7  font-semibold tracking-[-0.01em] text-main-50">
            {title}
          </h1>
          {subtitle ? (
            <span className="ml-1.5 truncate text-xs/4  text-main-400" title={subtitle}>
              {subtitle}
            </span>
          ) : null}
        </div>
        {actions ? <div className="flex flex-none items-center gap-3">{actions}</div> : null}
      </header>
      {toolbar ? (
        <div className="flex h-11.5 flex-none items-center gap-1.5 border-b border-main-750 bg-main-900 px-5">
          {toolbar}
        </div>
      ) : null}
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-4.5 px-5 py-4.5">
        {children}
      </ScrollArea>
    </div>
  );
}
