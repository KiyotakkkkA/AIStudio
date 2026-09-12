import type { Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { RunService } from "../services/RunService.ts";

export function createRunHandlers(
  service: RunService,
): Pick<
  IpcHandlers<Contract>,
  "runs.start" | "runs.cancel" | "runs.list" | "runs.get" | "runs.steps"
> {
  return {
    "runs.start": (input) => service.start(input),
    "runs.cancel": ({ id }) => service.cancel(id),
    "runs.list": () => service.list(),
    "runs.get": ({ id }) => service.get(id),
    "runs.steps": ({ id }) => service.steps(id),
  };
}
