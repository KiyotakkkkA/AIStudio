import { timestampNow, type Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { EventBus } from "../platform/events.ts";
import type { SystemService } from "../services/SystemService.ts";

export type SystemHandlers = Pick<
  IpcHandlers<Contract>,
  "system.ping" | "system.demoStream" | "system.nativePing"
>;

export interface SystemHandlerOptions {
  system: SystemService;
  events: EventBus;
  clock?: () => number;
  intervalMs?: number;
}

export function createSystemHandlers(options: SystemHandlerOptions): SystemHandlers {
  const clock = options.clock ?? Date.now;
  const intervalMs = options.intervalMs ?? 200;

  return {
    "system.nativePing": ({ text }) => options.system.nativePing(text),
    "system.ping": ({ sentAt }) => {
      const hostTime = timestampNow(clock);
      return { pong: true, hostTime, roundTripHint: hostTime - sentAt };
    },

    "system.demoStream": ({ steps }) => {
      const stream = options.events.openStream();
      let done = 0;
      const tick = (): void => {
        if (stream.closed) return;
        done += 1;
        stream.emit({ type: "progress", done, total: steps });
        if (done >= steps) stream.end({ status: "ok" });
        else setTimeout(tick, intervalMs);
      };
      setTimeout(tick, intervalMs);
      return { streamId: stream.id };
    },
  };
}
