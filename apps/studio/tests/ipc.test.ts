import { createProviderService } from "../../../test/helpers/providerService.ts";
import assert from "node:assert/strict";
import { afterAll, test } from "vitest";
import { contract } from "@zvs/shared";
import { createHandlers } from "../src/host/ipc/index.ts";
import { SettingService } from "../src/host/services/SettingService.ts";
import { createFakeClock } from "../../../test/helpers/fakeClock.ts";
import { temporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { createSecretService } from "../../../test/helpers/secretService.ts";

const database = temporaryDatabase();
const settings = new SettingService({ data: database.client });
const secrets = createSecretService(database.client);
afterAll(() => database.dispose());

test("system.ping echoes the host clock and matches the contract", () => {
  const handlers = createHandlers({
    providers: createProviderService(database.client),
    settings,
    secrets,
    clock: createFakeClock(1_700_000_000_500),
  });
  const output = handlers["system.ping"]({ sentAt: 1_700_000_000_000 });
  assert.deepEqual(output, {
    pong: true,
    hostTime: 1_700_000_000_500,
    roundTripHint: 500,
  });
  assert.deepEqual(contract["system.ping"].output.parse(output), output);
});

test("every contract channel has a handler at runtime too", () => {
  const handlers = createHandlers({
    providers: createProviderService(database.client),
    settings,
    secrets,
  });
  assert.deepEqual(Object.keys(handlers).sort(), Object.keys(contract).sort());
});
