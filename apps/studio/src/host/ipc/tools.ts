import { AppError, AppErrorCode, type Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { ToolInvoker } from "../tools/ToolInvoker.ts";
import type { ToolRegistry } from "../tools/ToolRegistry.ts";

type ToolChannels =
  | "tools.list"
  | "tools.get"
  | "tools.setTier"
  | "tools.setEnabled"
  | "tools.usage"
  | "tools.invoke"
  | "tools.defaults"
  | "tools.setDefaults";

export function createToolHandlers(
  tools?: ToolRegistry,
  invoker?: ToolInvoker,
): Pick<IpcHandlers<Contract>, ToolChannels> {
  const registry = (): ToolRegistry => {
    if (!tools) throw new AppError(AppErrorCode.CONFLICT, "Реестр инструментов недоступен");
    return tools;
  };
  const runner = (): ToolInvoker => {
    if (!invoker) throw new AppError(AppErrorCode.CONFLICT, "Запуск инструментов недоступен");
    return invoker;
  };
  return {
    "tools.list": (filter) => registry().list(filter),
    "tools.get": ({ id }) => registry().get(id),
    "tools.setTier": ({ id, tier }) => registry().setTier(id, tier),
    "tools.setEnabled": ({ id, enabled }) => registry().setEnabled(id, enabled),
    "tools.usage": ({ id }) => registry().usage(id),
    "tools.invoke": (input) => runner().invoke(input),
    "tools.defaults": () => registry().defaults(),
    "tools.setDefaults": (input) => registry().setDefaults(input),
  };
}
