import { createContext, useContext } from "react";

export interface CanvasActions {
  readonly duplicate: (id: string) => void;
  readonly remove: (id: string) => void;
}

const NOOP: CanvasActions = { duplicate: () => undefined, remove: () => undefined };

export const CanvasActionsContext = createContext<CanvasActions>(NOOP);

export function useCanvasActions(): CanvasActions {
  return useContext(CanvasActionsContext);
}
