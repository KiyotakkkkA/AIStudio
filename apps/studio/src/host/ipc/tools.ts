import { AppError, AppErrorCode, type Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { ToolRegistry } from "../tools/ToolRegistry.ts";

type ToolChannels =
  "tools.list" | "tools.get" | "tools.setTier" | "tools.setEnabled" | "tools.usage";

export function createToolHandlers(
  tools?: ToolRegistry,
): Pick<IpcHandlers<Contract>, ToolChannels> {
  const registry = (): ToolRegistry => {
    if (!tools) throw new AppError(AppErrorCode.CONFLICT, "Реестр инструментов недоступен");
    return tools;
  };
  return {
    "tools.list": (filter) => registry().list(filter),
    "tools.get": ({ id }) => registry().get(id),
    "tools.setTier": ({ id, tier }) => registry().setTier(id, tier),
    "tools.setEnabled": ({ id, enabled }) => registry().setEnabled(id, enabled),
    "tools.usage": ({ id }) => registry().usage(id),
  };
}
