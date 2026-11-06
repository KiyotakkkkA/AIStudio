import { observer } from "mobx-react-lite";
import type { ReactNode } from "react";
import { useStore } from "../../stores/useStore";

export default observer(function SettingRow({
  id,
  label,
  hint,
  error,
  stacked = false,
  footer,
  children,
}: {
  readonly id: string;
  readonly label: string;
  readonly hint: string;
  readonly error?: string | undefined;
  readonly stacked?: boolean;
  readonly footer?: ReactNode;
  readonly children: ReactNode;
}) {
  const { ui } = useStore();
  return (
    <div
      data-setting={id}
      className={`flex ${stacked ? "flex-col items-stretch" : "flex-wrap items-center"} gap-x-6 gap-y-2.5 border-b border-line py-4 transition-colors duration-500 last:border-b-0 ${
        ui.preferenceTarget === id ? "bg-accent-soft" : ""
      }`}
    >
      <div className={`flex min-w-0 flex-col gap-0.5 ${stacked ? "" : "min-w-56 flex-1"}`}>
        <span className="text-[13px] font-medium text-main-100">{label}</span>
        <span className="text-[12px] text-main-500">{hint}</span>
        {error === undefined ? null : (
          <span role="alert" className="text-[12px] text-err">
            {error}
          </span>
        )}
      </div>
      <div className={stacked ? "flex flex-col gap-2.5" : "flex flex-none items-center"}>
        {children}
      </div>
      {footer === undefined ? null : <div className="basis-full">{footer}</div>}
    </div>
  );
});
