import { useContext } from "react";
import type { RootStore } from "./RootStore";
import { StoreContext } from "./StoreContext";

export function useStore(): RootStore {
  const store = useContext(StoreContext);
  if (!store) throw new Error("Root store is not provided");
  return store;
}

export default useStore;
