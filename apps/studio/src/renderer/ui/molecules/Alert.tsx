import { mdiAlertCircleOutline, mdiAlertOutline, mdiClose, mdiInformationOutline } from "@mdi/js";
import type { ReactNode } from "react";
import Icon from "../atoms/Icon";
import IconButton from "../atoms/buttons/IconButton";

export type AlertTone = "error" | "warning" | "info";

const TONES: Record<
  AlertTone,
  { readonly icon: string; readonly box: string; readonly mark: string }
> = {
  error: { icon: mdiAlertCircleOutline, box: "border-err-border bg-err-soft", mark: "text-err" },
  warning: { icon: mdiAlertOutline, box: "border-warn-line bg-warn-soft", mark: "text-warn" },
  info: { icon: mdiInformationOutline, box: "border-info-line bg-info-soft", mark: "text-info" },
};

export const DISMISS_ALERT = "Скрыть";

export type AlertProps = {
  readonly tone?: AlertTone;
  readonly title?: ReactNode;
  readonly children?: ReactNode;
  readonly action?: ReactNode;
  readonly onDismiss?: () => void;
  readonly dismissLabel?: string;
  readonly bar?: boolean;
  readonly className?: string;
};

export default function Alert({
  tone = "error",
  title,
  children,
  action,
  onDismiss,
  dismissLabel = DISMISS_ALERT,
  bar = false,
  className = "",
}: AlertProps) {
  const { icon, box, mark } = TONES[tone];
  const frame = bar ? "border-x-0 border-t-0 px-4 py-2" : "rounded-control px-3 py-2.5";
  return (
    <div
      role={tone === "error" ? "alert" : "status"}
      className={`flex flex-none items-start gap-2.5 border text-[12.5px] leading-[18px] text-main-100 ${frame} ${box} ${className}`}
    >
      <Icon path={icon} size={15} className={`mt-[1.5px] flex-none ${mark}`} />
      <div className="min-w-0 flex-1 break-words">
        {title === undefined ? null : <p className="m-0 font-medium text-main-50">{title}</p>}
        {children === undefined ? null : (
          <div className={title === undefined ? "" : "mt-0.5 text-main-300"}>{children}</div>
        )}
      </div>
      {action === undefined ? null : (
        <div className="-my-[5px] flex flex-none items-center gap-1.5">{action}</div>
      )}
      {onDismiss === undefined ? null : (
        <IconButton
          path={mdiClose}
          label={dismissLabel}
          iconSize={14}
          onClick={onDismiss}
          className="-my-[3px] -mr-1 size-6"
        />
      )}
    </div>
  );
}
