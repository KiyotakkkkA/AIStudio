import { expect, expectTypeOf, it } from "vitest";
import { AppErrorCode, err, ok } from "@zvs/shared";
import { createIpcClient, createIpcServer, isIpcError, IpcError } from "../src/index.js";
import { createFakeIpcMain, createRecordingLogger, testContract } from "./harness.js";

async function rejection(promise: Promise<unknown>): Promise<IpcError> {
  try {
    await promise;
  } catch (thrown: unknown) {
    return thrown as IpcError;
  }
  throw new Error("Expected the call to reject");
}

function clientOver(response: unknown, validateResponses = true) {
  const sent: { channel: string; payload: unknown }[] = [];
  const client = createIpcClient(
    testContract,
    {
      call(channel, payload) {
        sent.push({ channel, payload });
        return Promise.resolve(response);
      },
    },
    { validateResponses },
  );
  return { client, sent };
}

it("resolves with the channel's output and infers both directions", async () => {
  const { client, sent } = clientOver(ok({ pong: true, hostTime: 11 }));
  const output = await client.call("system.ping", { sentAt: 10 });
  expectTypeOf(output).toEqualTypeOf<{ pong: true; hostTime: number }>();
  expect(output).toEqual({ pong: true, hostTime: 11 });
  expect(sent).toEqual([{ channel: "system.ping", payload: { sentAt: 10 } }]);
});

it("throws IpcError carrying the code on an err envelope", async () => {
  const { client } = clientOver(err(AppErrorCode.SECRET_MISSING, "Секрет не найден", { id: "7" }));
  const ipcError = await rejection(client.call("system.ping", { sentAt: 10 }));
  expect(isIpcError(ipcError)).toBe(true);
  expect(ipcError.code).toBe(AppErrorCode.SECRET_MISSING);
  expect(ipcError.channel).toBe("system.ping");
  expect(ipcError.message).toBe("Секрет не найден");
  expect(ipcError.details).toEqual({ id: "7" });
});

it("rejects invalid input before it reaches the bridge", async () => {
  const { client, sent } = clientOver(ok({ pong: true, hostTime: 11 }));
  const error = await rejection(client.call("system.ping", { sentAt: -1 }));
  expect(error.code).toBe(AppErrorCode.VALIDATION_FAILED);
  expect(sent).toEqual([]);
});

it("fails loudly on a drifted response envelope, and trusts it when validation is off", async () => {
  const drifted = { ok: true, data: { pong: true, hostTime: "later" } };
  const validating = clientOver(drifted);
  const error = await rejection(validating.client.call("system.ping", { sentAt: 10 }));
  expect(error.code).toBe(AppErrorCode.UNKNOWN);

  const trusting = clientOver(drifted, false);
  expect(await trusting.client.call("system.ping", { sentAt: 10 })).toEqual(drifted.data);
});

it("round-trips through the server across a bridge, error codes included", async () => {
  const ipcMain = createFakeIpcMain();
  createIpcServer(
    testContract,
    {
      "system.ping": (input) => ({ pong: true as const, hostTime: input.sentAt + 5 }),
      "system.echo": (input) => ({ text: input.text.toUpperCase() }),
    },
    { ipcMain, logger: createRecordingLogger() },
  );
  const client = createIpcClient(testContract, { call: (c, p) => ipcMain.invoke(c, p) });
  expect(await client.call("system.ping", { sentAt: 1 })).toEqual({ pong: true, hostTime: 6 });
  expect(await client.call("system.echo", { text: "ok" })).toEqual({ text: "OK" });
});

it("propagates a missing host handler as a rejection rather than hanging", async () => {
  const ipcMain = createFakeIpcMain();
  const client = createIpcClient(testContract, { call: (c, p) => ipcMain.invoke(c, p) });
  await expect(client.call("system.ping", { sentAt: 1 })).rejects.toThrow(/No handler registered/);
});
