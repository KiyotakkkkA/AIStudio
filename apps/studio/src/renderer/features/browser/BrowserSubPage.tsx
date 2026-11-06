import type { ReactNode } from "react";
import { mdiArrowLeft } from "@mdi/js";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Alert from "../../ui/molecules/Alert";
import { BACK } from "./browserDataCopy";

export default function BrowserSubPage({
  title,
  hint,
  actions,
  error,
  onDismissError,
  onBack,
  children,
}: {
  readonly title: string;
  readonly hint?: string;
  readonly actions?: ReactNode;
  readonly error?: string | null;
  readonly onDismissError?: () => void;
  readonly onBack: () => void;
  readonly children: ReactNode;
}) {
  return (
    <section aria-label={title} className="flex flex-col gap-4 pt-2">
      <div className="flex items-center gap-2">
        <IconButton path={mdiArrowLeft} label={BACK} size="sm" onClick={onBack} />
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <h3 className="m-0 text-[14px] font-semibold text-main-50">{title}</h3>
          {hint === undefined ? null : <p className="m-0 text-[12px] text-main-500">{hint}</p>}
        </div>
        {actions === undefined ? null : <div className="flex flex-none gap-2">{actions}</div>}
      </div>
      {error === null || error === undefined ? null : (
        <Alert onDismiss={onDismissError}>{error}</Alert>
      )}
      {children}
    </section>
  );
}
