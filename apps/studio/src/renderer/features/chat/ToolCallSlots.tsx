import { createContext, useContext, type ReactNode } from "react";
import type { BackgroundShellId } from "@zvs/shared";
import BackgroundShellLine from "../backgroundShells/BackgroundShellLine";
import ToolImages from "./ToolImages";
import type { toolImageReferences } from "./imageData";

export type ToolImageReferences = NonNullable<ReturnType<typeof toolImageReferences>>;

export interface ToolCallSlots {
  readonly images: (references: ToolImageReferences) => ReactNode;
  readonly shell: (id: BackgroundShellId) => ReactNode;
}

const STORE_SLOTS: ToolCallSlots = {
  images: (references) => <ToolImages references={references} />,
  shell: (id) => <BackgroundShellLine id={id} />,
};

export const ToolCallSlotsContext = createContext<ToolCallSlots>(STORE_SLOTS);

export function useToolCallSlots(): ToolCallSlots {
  return useContext(ToolCallSlotsContext);
}
