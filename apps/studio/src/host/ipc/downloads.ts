import { AppError, AppErrorCode, type Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { DownloadService } from "../downloads/DownloadService.ts";

type DownloadChannels =
  | "downloads.list"
  | "downloads.start"
  | "downloads.pause"
  | "downloads.resume"
  | "downloads.cancel"
  | "downloads.remove"
  | "downloads.catalogue"
  | "downloads.disk";

export function createDownloadHandlers(
  downloads?: DownloadService,
): Pick<IpcHandlers<Contract>, DownloadChannels> {
  const service = (): DownloadService => {
    if (!downloads) throw new AppError(AppErrorCode.CONFLICT, "Сервис загрузок недоступен");
    return downloads;
  };
  return {
    "downloads.list": (filter) => service().list(filter),
    "downloads.start": (input) => service().start(input),
    "downloads.pause": ({ id }) => service().pause(id),
    "downloads.resume": ({ id }) => service().resume(id),
    "downloads.cancel": ({ id }) => service().cancel(id),
    "downloads.remove": async ({ id }) => {
      await service().remove(id);
      return { id, removed: true };
    },
    "downloads.catalogue": (filter) => service().catalogue(filter),
    "downloads.disk": ({ refresh }) => service().disk(refresh),
  };
}
