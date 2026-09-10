import type { DatabaseClient } from "../../apps/studio/src/host/data/client.ts";
import { AccountService } from "../../apps/studio/src/host/services/AccountService.ts";
import { BrowserLifecycle } from "../../apps/studio/src/host/browser/lifecycle.ts";
import { createEventBus } from "../../apps/studio/src/host/platform/events.ts";
import { createSecretService } from "./secretService.ts";

export function createAccountService(data: DatabaseClient): AccountService {
  return new AccountService({
    data,
    secrets: createSecretService(data),
    lifecycle: new BrowserLifecycle(),
    events: createEventBus(),
    browser: {
      openTab() {
        throw new Error("Unexpected browser call");
      },
    },
    sessions: () => {
      throw new Error("Unexpected session call");
    },
  });
}
