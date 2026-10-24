import type { ReactNode } from "react";
import { motion } from "motion/react";
import { useFx, useTransition } from "./fx";

export default function FadeIn({
  className,
  children,
}: {
  readonly className?: string;
  readonly children: ReactNode;
}) {
  const transition = useTransition("reveal");
  const lift = useFx().motion === "full" ? 6 : 0;
  return (
    <motion.div
      initial={{ opacity: 0, y: lift }}
      animate={{ opacity: 1, y: 0 }}
      transition={transition}
      className={className}
    >
      {children}
    </motion.div>
  );
}
