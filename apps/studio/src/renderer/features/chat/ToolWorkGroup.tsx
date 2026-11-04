import { useId, useState, type ReactNode } from "react";
import { mdiChevronDown } from "@mdi/js";
import Icon from "../../ui/atoms/Icon";
import Collapse from "../../ui/motion/Collapse";
import { plural } from "../connections/connectionPresentation";
import { durationLabel } from "./processCopy";
import { TOOL_CALLS_LABEL } from "./toolCallCopy";

export const WORKED_LEAD = "Работа заняла";

export function workedDuration(milliseconds: number): string {
  if (milliseconds < 60_000) return durationLabel(milliseconds);
  const totalSeconds = Math.floor(milliseconds / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  const rest = seconds === 0 ? "" : ` ${seconds.toLocaleString("ru-RU")} с`;
  return `${minutes.toLocaleString("ru-RU")} мин${rest}`;
}

export function workGroupLabel(totalMs: number, count: number): string {
  if (totalMs > 0) return `${WORKED_LEAD} ${workedDuration(totalMs)}`;
  return `${TOOL_CALLS_LABEL} · ${plural(count, "вызов", "вызова", "вызовов")}`;
}

export default function ToolWorkGroup({
  label,
  children,
}: {
  readonly label: string;
  readonly children: ReactNode;
}) {
  const bodyId = useId();
  const [open, setOpen] = useState(false);
  return (
    <div className="border-b border-main-600/85">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={bodyId}
        onClick={() => setOpen(!open)}
        className="group/work flex min-h-8 w-full cursor-pointer items-center gap-2.5 py-1 text-left"
      >
        <span className="shrink-0 text-[12.5px] font-medium text-main-300 transition-colors group-hover/work:text-main-100">
          {label}
        </span>
        <span className="min-w-0 flex-1" />
        <Icon
          path={mdiChevronDown}
          size={15}
          className={`shrink-0 text-main-500 group-hover/work:text-main-200 anim-on:transition-transform ${
            open ? "rotate-180" : ""
          }`}
        />
      </button>
      <Collapse open={open} id={bodyId}>
        {children}
      </Collapse>
    </div>
  );
}
