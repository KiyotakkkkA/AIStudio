import { expect, it } from "vitest";
import { AppError, AppErrorCode, isOk } from "@zvs/shared";
import { createIpcServer, type IpcHandlers } from "../src/index.js";
import { createFakeIpcMain, createRecordingLogger, testContract } from "./harness.js";

type TestHandlers = IpcHandlers<typeof testContract>;

const handlers: TestHandlers = {
  "system.ping": (input: { sentAt: number }) => ({
    pong: true as const,
    hostTime: input.sentAt + 1,
  }),
  "system.echo": (input: { text: string }) => ({ text: input.text }),
};

function setup(overrides: Partial<TestHandlers> = {}, validateOutput = true) {
  const ipcMain = createFakeIpcMain();
  const logger = createRecordingLogger();
  const server = createIpcServer(
    testContract,
    { ...handlers, ...overrides },
    { ipcMain, logger, validateOutput },
  );
  return { ipcMain, logger, server };
}

it("registers every contract channel and disposes them", () => {
  const { ipcMain, server } = setup();
  expect(ipcMain.registered().sort()).toEqual(["system.echo", "system.ping"]);
  server.dispose();
  expect(ipcMain.registered()).toEqual([]);
});

it("round-trips a valid call", async () => {
  const { ipcMain } = setup();
  const result = await ipcMain.invoke("system.ping", { sentAt: 10 });
  expect(result).toEqual({ ok: true, data: { pong: true, hostTime: 11 } });
});

it("rejects invalid input with VALIDATION_FAILED before the handler runs", async () => {
  let calls = 0;
  const { ipcMain, logger } = setup({
    "system.ping": (input: { sentAt: number }) => {
      calls += 1;
      return { pong: true as const, hostTime: input.sentAt };
    },
  });
  const result = await ipcMain.invoke("system.ping", { sentAt: "not-a-number" });
  expect(result).toEqual({
    ok: false,
    error: { code: AppErrorCode.VALIDATION_FAILED, message: "Некорректные данные запроса" },
  });
  expect(calls).toBe(0);
  expect(logger.entries.at(0)?.message).toBe("Rejected invalid input");
});

it("maps a thrown AppError to its code and keeps details", async () => {
  const { ipcMain } = setup({
    "system.echo": () => {
      throw new AppError(AppErrorCode.NOT_FOUND, "Запись не найдена", { details: { id: "7" } });
    },
  });
  const result = await ipcMain.invoke("system.echo", { text: "hi" });
  expect(result).toEqual({
    ok: false,
    error: { code: AppErrorCode.NOT_FOUND, message: "Запись не найдена", details: { id: "7" } },
  });
});

it("maps an unknown thrown error to UNKNOWN and never sends the stack", async () => {
  const secret = "C:/host/only/secret-path.ts";
  const { ipcMain, logger } = setup({
    "system.echo": () => {
      throw new Error(`boom at ${secret}`);
    },
  });
  const result = await ipcMain.invoke("system.echo", { text: "hi" });
  expect(result).toEqual({
    ok: false,
    error: { code: AppErrorCode.UNKNOWN, message: "Внутренняя ошибка" },
  });
  expect(JSON.stringify(result)).not.toContain("boom");
  expect(JSON.stringify(result)).not.toContain(secret);
  const logged = logger.entries.find((entry) => entry.message === "Handler failed");
  expect(String(logged?.fields.error)).toContain("boom");
});

it("rejects a rejected promise the same way", async () => {
  const { ipcMain } = setup({
    "system.echo": () => Promise.reject(new AppError(AppErrorCode.DB_ERROR, "Сбой базы данных")),
  });
  const result = await ipcMain.invoke("system.echo", { text: "hi" });
  expect(isOk(result as never)).toBe(false);
  expect(result).toMatchObject({ error: { code: AppErrorCode.DB_ERROR } });
});

it("catches handler output that drifts from the contract when validation is on", async () => {
  const drifting = { "system.echo": () => ({ text: 42 }) as unknown as { text: string } };
  const validating = setup(drifting, true);
  expect(await validating.ipcMain.invoke("system.echo", { text: "hi" })).toEqual({
    ok: false,
    error: { code: AppErrorCode.UNKNOWN, message: "Внутренняя ошибка" },
  });
  expect(
    validating.logger.entries.some(
      (entry) => entry.message === "Handler output does not match the contract",
    ),
  ).toBe(true);

  const trusting = setup(drifting, false);
  expect(await trusting.ipcMain.invoke("system.echo", { text: "hi" })).toEqual({
    ok: true,
    data: { text: 42 },
  });
});
