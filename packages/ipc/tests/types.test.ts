import { expect, it } from "vitest";
import { createIpcServer } from "../src/index.js";
import { createFakeIpcMain, createRecordingLogger, testContract } from "./harness.js";

it("rejects an incomplete or mistyped handler map at compile time", () => {
  const options = { ipcMain: createFakeIpcMain(), logger: createRecordingLogger() };

  createIpcServer(
    testContract,
    // @ts-expect-error 'system.echo' is missing from the handler map.
    {
      "system.ping": (input: { sentAt: number }) => ({
        pong: true as const,
        hostTime: input.sentAt,
      }),
    },
    options,
  );

  createIpcServer(
    testContract,
    {
      "system.ping": (input) => ({ pong: true as const, hostTime: input.sentAt }),
      // @ts-expect-error the output must match the channel's schema.
      "system.echo": () => ({ text: 42 }),
    },
    options,
  );

  createIpcServer(
    testContract,
    {
      "system.ping": (input) => ({ pong: true as const, hostTime: input.sentAt }),
      "system.echo": (input) => ({ text: input.text }),
      // @ts-expect-error a channel outside the contract cannot be registered.
      "system.nope": () => ({ text: "no" }),
    },
    options,
  );

  expect(true).toBe(true);
});
