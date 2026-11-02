import { mdiChevronDown } from "@mdi/js";
import { useId, useState, type ReactNode } from "react";
import Icon from "../../ui/atoms/Icon";
import Collapse from "../../ui/motion/Collapse";

function Disclosure({
  label,
  hint,
  aside,
  defaultOpen = false,
  className,
  children,
}: {
  readonly label: string;
  readonly hint?: string;
  readonly aside?: ReactNode;
  readonly defaultOpen?: boolean;
  readonly className?: string;
  readonly children: ReactNode;
}) {
  const bodyId = useId();
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className={className}>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={bodyId}
        onClick={() => {
          setOpen(!open);
        }}
        className="group flex min-h-9 w-full cursor-pointer items-center gap-2.5 py-1.5 text-left"
      >
        <span className="shrink-0 text-[13px] font-medium text-main-100">{label}</span>
        <span className="min-w-0 flex-1 truncate text-[12px] text-main-500">{hint ?? ""}</span>
        {aside}
        <Icon
          path={mdiChevronDown}
          size={16}
          className={`shrink-0 text-main-500 group-hover:text-main-200 anim-on:transition-transform ${
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

export default Disclosure;
