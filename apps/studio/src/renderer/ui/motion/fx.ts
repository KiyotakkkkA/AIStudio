import { createContext, useContext } from "react";
import type { Transition } from "motion/react";
import type { FxLevel } from "./levels";

export interface FxLevels {
  readonly motion: FxLevel;
  readonly effects: FxLevel;
}

export const FxContext = createContext<FxLevels>({ motion: "full", effects: "full" });

export function useFx(): FxLevels {
  return useContext(FxContext);
}

export type TransitionKind = "layout" | "reveal" | "pop";

const INSTANT: Transition = { duration: 0 };

const REDUCED: Transition = { duration: 0.16, ease: [0.2, 0, 0, 1] };

const FULL: Record<TransitionKind, Transition> = {
  layout: { type: "spring", bounce: 0.18, duration: 0.38 },
  reveal: { type: "spring", bounce: 0, duration: 0.34 },
  pop: { type: "spring", bounce: 0.45, duration: 0.42 },
};

export function transitionFor(level: FxLevel, kind: TransitionKind): Transition {
  if (level === "none") return INSTANT;
  if (level === "reduced") return REDUCED;
  return FULL[kind];
}

export function useTransition(kind: TransitionKind): Transition {
  return transitionFor(useFx().motion, kind);
}
