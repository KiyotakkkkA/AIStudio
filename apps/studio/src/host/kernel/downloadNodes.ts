import { AppError, AppErrorCode, DownloadFetchInput, DownloadReportDto } from "@zvs/shared";
import type { NodeRegistry } from "./NodeRegistry.ts";

/**
 * One queue, two views: a download is a kernel run like any other, so it shows on the Tasks
 * page with the same progress, cancel and history as an indexing job. The Downloads page is a
 * specialised view over these same runs with its own richer per-item state.
 */
export function registerDownloadNodes(registry: NodeRegistry): NodeRegistry {
  registry.register({
    type: "download.fetch",
    input: DownloadFetchInput,
    output: DownloadReportDto,
    permission: { tool: "download.fetch" },
    sideEffect: true,
    async run(context, input) {
      context.signal.throwIfAborted();
      const downloads = context.services.downloads;
      if (!downloads) throw new AppError(AppErrorCode.CONFLICT, "Сервис загрузок недоступен");
      return downloads.execute(input.downloadId, {
        signal: context.signal,
        // The smoothed rate belongs to the Downloads page, which reads it from `downloads.list`;
        // putting it on the run stream would log a line several times a second.
        onProgress: ({ done, total }) => {
          context.emit({ type: "progress", done, total });
        },
      });
    },
  });
  return registry;
}
