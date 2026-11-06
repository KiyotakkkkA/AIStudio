import { useId, type ReactNode } from "react";

export interface PreferenceContainerProps {
  readonly title: string;
  readonly hint?: string;
  readonly action?: ReactNode;
  readonly bare?: boolean;
  readonly children: ReactNode;
}

export default function PreferenceContainer({
  title,
  hint,
  action,
  bare = false,
  children,
}: PreferenceContainerProps) {
  const titleId = useId();
  return (
    <section aria-labelledby={titleId} className="flex h-fit flex-col gap-3">
      <div className="flex items-end gap-3">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <h3 id={titleId} className="m-0 text-[13px] font-medium text-main-100">
            {title}
          </h3>
          {hint === undefined ? null : <p className="m-0 text-[12px] text-main-500">{hint}</p>}
        </div>
        {action === undefined ? null : <div className="flex flex-none items-center">{action}</div>}
      </div>
      {bare ? (
        children
      ) : (
        <div className="rounded-card border border-line bg-main-850 px-4">{children}</div>
      )}
    </section>
  );
}
