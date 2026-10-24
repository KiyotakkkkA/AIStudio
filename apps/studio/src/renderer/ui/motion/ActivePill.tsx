import { motion } from "motion/react";
import { useTransition } from "./fx";

export default function ActivePill({
  layoutId,
  className,
}: {
  readonly layoutId: string;
  readonly className: string;
}) {
  const transition = useTransition("layout");
  return (
    <motion.span
      layoutId={layoutId}
      transition={transition}
      aria-hidden="true"
      className={`pointer-events-none absolute inset-0 ${className}`}
    />
  );
}
