import type { ReactNode } from "react";
import type { RootStore } from "../stores/RootStore";
import { StoreContext } from "../stores/StoreContext";

export interface StoreProviderProps {
  readonly store: RootStore;
  readonly children: ReactNode;
}

export default function StoreProvider({ store, children }: StoreProviderProps) {
  return <StoreContext value={store}>{children}</StoreContext>;
}
