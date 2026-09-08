import { useEffect, useState, type ReactNode } from "react";
import { createRootStore, type RootStore, type RootStoreEnvironment } from "../stores/RootStore";
import { StoreContext } from "../stores/StoreContext";

export interface StoreProviderProps {
  readonly environment: RootStoreEnvironment;
  readonly children: ReactNode;
}

export default function StoreProvider({ environment, children }: StoreProviderProps) {
  const [store] = useState<RootStore>(() => createRootStore(environment));

  useEffect(() => {
    void store.boot();
  }, [store]);

  return <StoreContext value={store}>{children}</StoreContext>;
}
