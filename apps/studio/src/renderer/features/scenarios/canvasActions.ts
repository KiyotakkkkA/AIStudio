import { createContext, useContext } from "react";
import type { PaletteAnchor } from "./palette";

export interface CanvasActions {
  readonly duplicate: (id: string) => void;
  readonly remove: (id: string) => void;
  readonly next: (anchor: PaletteAnchor) => void;
}

const NOOP: CanvasActions = {
  duplicate: () => undefined,
  remove: () => undefined,
  next: () => undefined,
};

export const CanvasActionsContext = createContext<CanvasActions>(NOOP);

export function useCanvasActions(): CanvasActions {
  return useContext(CanvasActionsContext);
}
