import type { Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { AccountService } from "../services/AccountService.ts";

export function createAccountHandlers(
  accounts: AccountService,
): Pick<
  IpcHandlers<Contract>,
  "accounts.list" | "accounts.link" | "accounts.cancelLink" | "accounts.unlink" | "accounts.refresh"
> {
  return {
    "accounts.list": () => accounts.list(),
    "accounts.link": (input) => accounts.link(input),
    "accounts.cancelLink": (input) => accounts.cancelLink(input),
    "accounts.unlink": ({ id }) => accounts.unlink(id),
    "accounts.refresh": ({ id }) => accounts.refresh(id),
  };
}
