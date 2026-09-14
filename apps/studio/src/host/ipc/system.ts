import { timestampNow, type Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { EventBus } from "../platform/events.ts";
import type { RunService } from "../services/RunService.ts";
import type { SystemService } from "../services/SystemService.ts";

export type SystemHandlers = Pick<
  IpcHandlers<Contract>,
  "system.ping" | "system.demoStream" | "system.demoJob" | "system.nativePing" | "system.device"
>;

export interface SystemHandlerOptions {
  system: SystemService;
  runs: Pick<RunService, "start">;
  events: EventBus;
  clock?: () => number;
  intervalMs?: number;
}

export function createSystemHandlers(options: SystemHandlerOptions): SystemHandlers {
  const clock = options.clock ?? Date.now;
  const intervalMs = options.intervalMs ?? 200;

  return {
    "system.nativePing": ({ text }) => options.system.nativePing(text),
    "system.device": ({ refresh }) => options.system.deviceProfile(refresh),
    "system.ping": ({ sentAt }) => {
      const hostTime = timestampNow(clock);
      return { pong: true, hostTime, roundTripHint: hostTime - sentAt };
    },

    // The permanent smoke test for the sidecar: one kernel run over one `job.sleep`, so the
    // whole path — spawn, protocol, progress, cancel — is visible on the Tasks page.
    "system.demoJob": ({ steps, intervalMs: step }) =>
      options.runs.start({
        kind: "job",
        title: "Демонстрационная задача",
        graph: {
          nodes: [
            {
              id: "sleep",
              type: "job.run",
              dependencies: [],
              bindings: {},
              retry: { maxAttempts: 1, backoffMs: 0 },
              input: {
                job: "job.sleep",
                params: { steps, ...(step === undefined ? {} : { intervalMs: step }) },
              },
            },
          ],
        },
        input: null,
        concurrency: 1,
      }),

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
