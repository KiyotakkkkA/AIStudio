import type { Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { RunService } from "../services/RunService.ts";

export function createRunHandlers(
  service: RunService,
): Pick<
  IpcHandlers<Contract>,
  | "runs.approve"
  | "runs.deny"
  | "runs.start"
  | "runs.cancel"
  | "runs.list"
  | "runs.get"
  | "runs.steps"
> {
  return {
    "runs.approve": ({ id, approvalId, always }) => service.approve(id, approvalId, always),
    "runs.deny": ({ id, approvalId }) => service.deny(id, approvalId),
    "runs.start": (input) => service.start(input),
    "runs.cancel": ({ id }) => service.cancel(id),
    "runs.list": () => service.list(),
    "runs.get": ({ id }) => service.get(id),
    "runs.steps": ({ id }) => service.steps(id),
  };
}
