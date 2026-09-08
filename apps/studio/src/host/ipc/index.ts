import type { Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import { createSystemHandlers } from "./system.ts";

export interface HostIpcDependencies {
  clock?: () => number;
}

export function createHandlers(dependencies: HostIpcDependencies = {}): IpcHandlers<Contract> {
  return {
    ...createSystemHandlers(dependencies.clock),
  };
}
