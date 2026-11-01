import type { CSSProperties, ReactNode } from "react";
import { motion } from "motion/react";
import Icon from "../../ui/atoms/Icon";
import { useFx } from "../../ui/motion/fx";

export const PIPELINE_LABEL = "Размышления и вызовы инструментов";

export function PipelineList({
  label,
  className,
  children,
}: {
  readonly label: string;
  readonly className?: string;
  readonly children: ReactNode;
}) {
  return (
    <ol aria-label={label} className={`m-0 list-none p-0 ${className ?? ""}`}>
      {children}
    </ol>
  );
}

export function PipelineItem({
  icon,
  iconClassName,
  enter = false,
  layout = false,
  tail = false,
  style,
  children,
}: {
  readonly icon: string;
  readonly iconClassName?: string;
  readonly enter?: boolean;
  readonly layout?: "position" | false;
  readonly tail?: boolean;
  readonly style?: CSSProperties;
  readonly children: ReactNode;
}) {
  const { motion: level } = useFx();
  return (
    <motion.li
      layout={level === "full" && layout !== false ? layout : false}
      initial={!enter || level === "none" ? false : { opacity: 0, y: level === "full" ? 6 : 0 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.22 }}
      style={style}
      className="relative list-none pb-1 pl-9"
    >
      <span
        aria-hidden="true"
        className="absolute top-0 left-2.75 h-1 border-l border-dashed border-line [li:first-child>&]:hidden"
      />
      <span
        aria-hidden="true"
        className={`absolute top-7 bottom-0 left-2.75 border-l border-dashed border-line ${
          tail ? "" : "[li:last-child>&]:hidden"
        }`}
      />
      <span
        aria-hidden="true"
        className={`absolute top-1 left-0 flex size-6 items-center justify-center rounded-md border border-line bg-main-850 text-main-400 ${iconClassName ?? ""}`}
      >
        <Icon path={icon} size={13} />
      </span>
      {children}
    </motion.li>
  );
}
