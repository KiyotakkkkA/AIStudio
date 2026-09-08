import { timestampNow, type Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";

export type SystemHandlers = Pick<IpcHandlers<Contract>, "system.ping">;

export function createSystemHandlers(clock: () => number = Date.now): SystemHandlers {
  return {
    "system.ping": ({ sentAt }) => {
      const hostTime = timestampNow(clock);
      return { pong: true, hostTime, roundTripHint: hostTime - sentAt };
    },
  };
}
