import assert from "node:assert/strict";
import test, { after } from "node:test";
import { contract } from "@zvs/shared";
import { createHandlers } from "../src/host/ipc/index.ts";
import { SettingService } from "../src/host/services/SettingService.ts";
import { temporaryDatabase } from "./database.ts";

const database = temporaryDatabase();
const settings = new SettingService({ data: database.client });
after(() => database.dispose());

test("system.ping echoes the host clock and matches the contract", () => {
  const handlers = createHandlers({ settings, clock: () => 1_700_000_000_500 });
  const output = handlers["system.ping"]({ sentAt: 1_700_000_000_000 });
  assert.deepEqual(output, {
    pong: true,
    hostTime: 1_700_000_000_500,
    roundTripHint: 500,
  });
  assert.deepEqual(contract["system.ping"].output.parse(output), output);
});

test("every contract channel has a handler at runtime too", () => {
  const handlers = createHandlers({ settings });
  assert.deepEqual(Object.keys(handlers).sort(), Object.keys(contract).sort());
});
