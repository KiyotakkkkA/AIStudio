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
  | "runs.detail"
  | "runs.retry"
  | "runs.clearFinished"
> {
  return {
    "runs.approve": ({ id, approvalId, always }) => service.approve(id, approvalId, always),
    "runs.deny": ({ id, approvalId }) => service.deny(id, approvalId),
    "runs.start": (input) => service.start(input),
    "runs.cancel": ({ id }) => service.cancel(id),
    "runs.list": (filter) => service.list(filter),
    "runs.get": ({ id }) => service.get(id),
    "runs.steps": ({ id }) => service.steps(id),
    "runs.detail": ({ id }) => service.detail(id),
    "runs.retry": ({ id }) => service.retry(id),
    "runs.clearFinished": () => service.clearFinished(),
  };
}
