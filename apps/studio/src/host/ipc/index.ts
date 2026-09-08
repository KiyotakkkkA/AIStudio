import type { Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import { createEventBus, type EventBus } from "../platform/events.ts";
import { createSystemHandlers } from "./system.ts";

export interface HostIpcDependencies {
  events?: EventBus;
  clock?: () => number;
  intervalMs?: number;
}

export function createHandlers(dependencies: HostIpcDependencies = {}): IpcHandlers<Contract> {
  return {
    ...createSystemHandlers({
      events: dependencies.events ?? createEventBus(),
      clock: dependencies.clock,
      intervalMs: dependencies.intervalMs,
    }),
  };
}
