import type { UnitOfWork } from "../../apps/studio/src/host/data/UnitOfWork.ts";
import { NodeRegistry } from "../../apps/studio/src/host/kernel/NodeRegistry.ts";
import { registerCoreNodes } from "../../apps/studio/src/host/kernel/coreNodes.ts";
import { createEventBus } from "../../apps/studio/src/host/platform/events.ts";
import { ChatService } from "../../apps/studio/src/host/services/ChatService.ts";
import { RunService } from "../../apps/studio/src/host/services/RunService.ts";

export function createChatService(data: UnitOfWork): ChatService {
  const registry = registerCoreNodes(new NodeRegistry());
  const runs = new RunService({ data, registry, events: createEventBus() });
  return new ChatService({ data, registry, runs });
}
