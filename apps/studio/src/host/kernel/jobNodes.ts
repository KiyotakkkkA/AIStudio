import { z } from "zod";
import { AppError, AppErrorCode } from "@zvs/shared";
import type { NodeRegistry } from "./NodeRegistry.ts";

export const JobRunInput = z.object({
  job: z.string().min(1).max(128),
  params: z.json().default(null),
});

/**
 * Bridges the sidecar into the kernel: a `job.run` step forwards the sidecar's progress as
 * `progress` events on the run's stream, so a sidecar job appears on the Tasks page like any
 * other run. Cancelling the run cancels the job through the same step context signal.
 */
export function registerJobNodes(registry: NodeRegistry): NodeRegistry {
  registry.register({
    type: "job.run",
    input: JobRunInput,
    output: z.json(),
    permission: { tool: "job.run" },
    sideEffect: true,
    async run(context, input) {
      context.signal.throwIfAborted();
      const jobs = context.services.jobs;
      if (!jobs) throw new AppError(AppErrorCode.CONFLICT, "Сервис задач недоступен");
      return jobs.run(input.job, input.params, {
        signal: context.signal,
        onProgress: ({ done, total, message }) => {
          context.emit({ type: "progress", done, total });
          if (message !== undefined)
            context.emit({ type: "log", line: { job: input.job, message } });
        },
      });
    },
  });
  return registry;
}
