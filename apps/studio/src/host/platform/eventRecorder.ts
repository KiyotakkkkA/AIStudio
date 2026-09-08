import { appendFileSync, mkdirSync } from "node:fs";
import type { EventRecorder } from "./events.ts";
import type { Logger } from "./logger.ts";
import { streamFilePath } from "./paths.ts";

export const RECORD_FLAG = "ZVS_RECORD_EVENTS";

export function recordingEnabled(env: NodeJS.ProcessEnv = process.env): boolean {
  return env[RECORD_FLAG] === "1";
}

export function createEventRecorder(options: {
  directory: string;
  logger?: Logger;
  clock?: () => number;
}): EventRecorder {
  const clock = options.clock ?? Date.now;
  mkdirSync(options.directory, { recursive: true });
  let closed = false;
  return {
    append(event) {
      if (closed) return;
      const line = JSON.stringify(event) + "\n";
      try {
        appendFileSync(streamFilePath(options.directory, clock()), line, "utf8");
      } catch (error: unknown) {
        closed = true;
        options.logger?.log("error", "events", "Stopped recording, the file is unwritable", {
          directory: options.directory,
          error: String(error),
        });
      }
    },
    close() {
      closed = true;
    },
  };
}
