import { createContext } from "react";
import type { RootStore } from "./RootStore";

export const StoreContext = createContext<RootStore | undefined>(undefined);
