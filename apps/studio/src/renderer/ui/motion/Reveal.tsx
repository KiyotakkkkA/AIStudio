import type { ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import { useFx, useTransition } from "./fx";

export default function Reveal({
  show,
  className,
  children,
}: {
  readonly show: boolean;
  readonly className?: string;
  readonly children: ReactNode;
}) {
  const transition = useTransition("reveal");
  const { motion: level } = useFx();
  const lift = level === "full" ? 8 : 0;
  return (
    <AnimatePresence initial={false}>
      {show ? (
        <motion.div
          initial={{ opacity: 0, y: lift, height: 0 }}
          animate={{ opacity: 1, y: 0, height: "auto" }}
          exit={{ opacity: 0, y: lift / 2, height: 0 }}
          transition={transition}
          className={`overflow-hidden ${className ?? ""}`}
        >
          {children}
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
