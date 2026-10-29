import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { AppErrorCode, contract, type CliInstallDto, type Contract } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import CliStore from "../../src/renderer/features/preferences/CliStore.ts";
import {
  CLI_INSTALL_FAILED,
  CLI_NEW_TERMINAL,
  CLI_NOT_BUNDLED,
  CLI_REMOVED,
} from "../../src/renderer/features/preferences/cliCopy.ts";

function status(installed: boolean, available = true): CliInstallDto {
  return { available, installed, command: "aistudio" };
}

test("installing refreshes the settings and uninstalling reports the removal", async () => {
  let refreshed = 0;
  const bridge = createFakeBridge<Contract>()
    .handle("cli.status", () => status(false))
    .handle("cli.install", () => status(true))
    .handle("cli.uninstall", () => status(false));
  const store = new CliStore(createIpcClient(contract, bridge), async () => {
    refreshed += 1;
  });

  await store.load();
  expect(store.installed).toBe(false);
  await store.install();
  expect(store.installed).toBe(true);
  expect(store.notice).toBe(CLI_NEW_TERMINAL);
  expect(refreshed).toBe(1);

  await store.uninstall();
  expect(store.installed).toBe(false);
  expect(store.notice).toBe(CLI_REMOVED);
  expect(refreshed).toBe(1);
});

test("a failed install maps the error code to copy", async () => {
  const conflict = createFakeBridge<Contract>().fail("cli.install", AppErrorCode.CONFLICT, "x");
  const missing = new CliStore(createIpcClient(contract, conflict));
  await missing.install();
  expect(missing.error).toBe(CLI_NOT_BUNDLED);

  const native = createFakeBridge<Contract>().fail("cli.install", AppErrorCode.NATIVE_ERROR, "x");
  const failed = new CliStore(createIpcClient(contract, native));
  await failed.install();
  expect(failed.error).toBe(CLI_INSTALL_FAILED);
  expect(failed.busy).toBe(false);
});
