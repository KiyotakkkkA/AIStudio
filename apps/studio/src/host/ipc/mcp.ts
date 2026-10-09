import { AppError, AppErrorCode, type Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { McpService } from "../services/McpService.ts";

type McpChannels =
  | "mcp.list"
  | "mcp.get"
  | "mcp.create"
  | "mcp.update"
  | "mcp.remove"
  | "mcp.start"
  | "mcp.stop"
  | "mcp.test"
  | "mcp.health";

export function createMcpHandlers(mcp?: McpService): Pick<IpcHandlers<Contract>, McpChannels> {
  const service = (): McpService => {
    if (!mcp) throw new AppError(AppErrorCode.CONFLICT, "Сервис MCP недоступен");
    return mcp;
  };
  return {
    "mcp.list": () => service().list(),
    "mcp.get": ({ id }) => service().get(id),
    "mcp.create": (input) => service().create(input),
    "mcp.update": (input) => service().update(input),
    "mcp.remove": async ({ id }) => {
      await service().remove(id);
      return { id, removed: true as const };
    },
    "mcp.start": async ({ id }) => {
      try {
        await service().start(id);
      } catch {
        return service().get(id);
      }
      return service().get(id);
    },
    "mcp.stop": async ({ id }) => {
      await service().stop(id);
      return service().get(id);
    },
    "mcp.test": ({ id }) => service().test(id),
    "mcp.health": ({ id }) => service().health(id),
  };
}
