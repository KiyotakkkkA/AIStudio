import type { DatabaseClient } from "../../apps/studio/src/host/data/client.ts";
import { GraphService } from "../../apps/studio/src/host/services/GraphService.ts";
import { FakeGraphCore } from "./FakeGraphCore.ts";

export function createGraphService(data: DatabaseClient, core = new FakeGraphCore()): GraphService {
  return new GraphService({ data, core });
}
