import { timestampNow, type Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { EventBus } from "../platform/events.ts";

export type SystemHandlers = Pick<IpcHandlers<Contract>, "system.ping" | "system.demoStream">;

export interface SystemHandlerOptions {
  events: EventBus;
  clock?: () => number;
  intervalMs?: number;
}

export function createSystemHandlers(options: SystemHandlerOptions): SystemHandlers {
  const clock = options.clock ?? Date.now;
  const intervalMs = options.intervalMs ?? 200;

  return {
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
