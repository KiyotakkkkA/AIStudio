import type { UnitOfWork } from "../../apps/studio/src/host/data/UnitOfWork.ts";
import { RunService } from "../../apps/studio/src/host/services/RunService.ts";
import { NodeRegistry } from "../../apps/studio/src/host/kernel/NodeRegistry.ts";
import { createEventBus } from "../../apps/studio/src/host/platform/events.ts";

export function createRunService(data: UnitOfWork): RunService {
  return new RunService({ data, registry: new NodeRegistry(), events: createEventBus() });
}
