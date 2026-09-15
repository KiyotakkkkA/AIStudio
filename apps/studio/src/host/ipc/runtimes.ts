import { AppError, AppErrorCode, type Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { RuntimeService } from "../runtimes/RuntimeService.ts";

type RuntimeChannel = "runtimes.list" | "runtimes.install" | "runtimes.stop" | "runtimes.remove";

function unavailable(): never {
  throw new AppError(AppErrorCode.CONFLICT, "Сервис локальных движков недоступен");
}

export function createRuntimeHandlers(
  service?: RuntimeService,
): Pick<IpcHandlers<Contract>, RuntimeChannel> {
  return {
    "runtimes.list": () => (service ?? unavailable()).overview(),
    "runtimes.install": (input) => (service ?? unavailable()).install(input),
    "runtimes.stop": (input) => ({ stopped: (service ?? unavailable()).stop(input.id) }),
    "runtimes.remove": async (input) => {
      await (service ?? unavailable()).uninstall(input.id);
      return { id: input.id, removed: true as const };
    },
  };
}
