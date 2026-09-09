import type { DatabaseClient } from "../../apps/studio/src/host/data/client.ts";
import { ProviderService } from "../../apps/studio/src/host/services/ProviderService.ts";
import { createFakeDriver } from "./FakeDriver.ts";
import { createSecretService } from "./secretService.ts";

export function createProviderService(data: DatabaseClient): ProviderService {
  return new ProviderService({
    data,
    secrets: createSecretService(data),
    drivers: { ephemeralDriver: async () => createFakeDriver(), invalidate() {} },
  });
}
