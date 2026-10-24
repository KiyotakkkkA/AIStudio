import type { ReactNode } from "react";
import { motion } from "motion/react";
import { useTransition } from "./fx";

export default function Collapse({
  open,
  id,
  className,
  children,
}: {
  readonly open: boolean;
  readonly id?: string;
  readonly className?: string;
  readonly children: ReactNode;
}) {
  const transition = useTransition("reveal");
  return (
    <motion.div
      id={id}
      initial={false}
      animate={open ? { height: "auto", opacity: 1 } : { height: 0, opacity: 0 }}
      transition={transition}
      inert={!open}
      aria-hidden={!open}
      className={`overflow-hidden ${className ?? ""}`}
    >
      {children}
    </motion.div>
  );
}
