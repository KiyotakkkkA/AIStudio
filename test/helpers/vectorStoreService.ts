import type { DatabaseClient } from "../../apps/studio/src/host/data/client.ts";
import { VectorStoreService } from "../../apps/studio/src/host/services/VectorStoreService.ts";
import { FakeVectorCore } from "./FakeVectorCore.ts";
import { createFakeDriver } from "./FakeDriver.ts";
import { STUDIO_ROOT } from "./paths.ts";

export function createVectorStoreService(data: DatabaseClient): VectorStoreService {
  return new VectorStoreService({
    data,
    core: new FakeVectorCore(),
    directory: STUDIO_ROOT,
    drivers: { ephemeralDriver: async () => createFakeDriver() },
  });
}
