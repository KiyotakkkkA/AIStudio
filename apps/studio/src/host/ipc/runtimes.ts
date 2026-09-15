import { AppError, AppErrorCode, type Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { RuntimeService } from "../runtimes/RuntimeService.ts";

type RuntimeChannel = "runtimes.list" | "runtimes.install" | "runtimes.stop";

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
  };
}
