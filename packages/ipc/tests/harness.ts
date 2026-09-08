import { z } from "zod";
import { defineContract } from "@zvs/shared";
import type { IpcMainLike, IpcServerLogger } from "../src/index.js";

export const testContract = defineContract({
  "system.ping": {
    input: z.object({ sentAt: z.number().int().nonnegative() }),
    output: z.object({ pong: z.literal(true), hostTime: z.number().int().nonnegative() }),
  },
  "system.echo": {
    input: z.object({ text: z.string() }),
    output: z.object({ text: z.string() }),
  },
});

export interface FakeIpcMain extends IpcMainLike {
  invoke(channel: string, payload: unknown): Promise<unknown>;
  registered(): string[];
}

export function createFakeIpcMain(): FakeIpcMain {
  const listeners = new Map<string, (event: unknown, ...args: unknown[]) => unknown>();
  return {
    handle(channel, listener) {
      listeners.set(channel, listener);
    },
    removeHandler(channel) {
      listeners.delete(channel);
    },
    async invoke(channel, payload) {
      const listener = listeners.get(channel);
      if (!listener) throw new Error(`No handler registered for '${channel}'`);
      return await listener({ senderId: 1 }, payload);
    },
    registered() {
      return [...listeners.keys()];
    },
  };
}

export interface RecordedLog {
  level: string;
  scope: string;
  message: string;
  fields: Record<string, unknown>;
}

export function createRecordingLogger(): IpcServerLogger & { entries: RecordedLog[] } {
  const entries: RecordedLog[] = [];
  return {
    entries,
    log(level, scope, message, fields = {}) {
      entries.push({ level, scope, message, fields });
    },
  };
}
