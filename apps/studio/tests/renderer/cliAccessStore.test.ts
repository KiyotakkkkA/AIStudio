import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  CLI_STEP_DOMAIN,
  contract,
  type CliConnectionDto,
  type CliConsentDto,
  type Contract,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import CliAccessStore from "../../src/renderer/features/preferences/CliAccessStore.ts";
import {
  CLI_CLIENT_RECOGNISED,
  CLI_CLIENT_UNKNOWN,
  CLI_DISCONNECT_FAILED,
  cliClientLine,
} from "../../src/renderer/features/preferences/cliCopy.ts";
import {
  CLI_CONSENT_FAILED,
  CLI_CONSENT_RULE_ANYWHERE,
  CLI_CONSENT_SOURCE_RECOGNISED,
  CLI_CONSENT_SOURCE_UNKNOWN,
  cliChangeText,
  cliSourceText,
} from "../../src/renderer/features/preferences/cliConsentCopy.ts";
import { EXEC_MODE_COPY } from "../../src/renderer/features/execPolicy/execPolicyCopy.ts";

const CONSENT = "9b2f7a3e-4c1d-4e8a-9f2b-6d5c4b3a2e10";
const STREAM = "6f1d2c3b-4a59-4687-9766-554433221100";

function consent(overrides: Partial<CliConsentDto> = {}): CliConsentDto {
  return {
    id: CONSENT,
    connectionId: 1,
    change: { kind: "mode", from: "always-ask", to: "no-restrictions" },
    recognised: true,
    image: null,
    createdAt: 1,
    expiresAt: 60_001,
    ...overrides,
  };
}

function connection(overrides: Partial<CliConnectionDto> = {}): CliConnectionDto {
  return {
    id: 1,
    pid: 4120,
    image: "C:/ZVS/cli/aistudio.exe",
    recognised: true,
    connectedAt: 1,
    dialogs: 2,
    ...overrides,
  };
}

function rig() {
  const bridge = createFakeBridge<Contract>();
  const events = createEventRouter();
  let consents: CliConsentDto[] = [consent()];
  let connections: CliConnectionDto[] = [connection()];
  const answers: unknown[] = [];
  bridge
    .handle("cli.consents.list", () => [...consents])
    .handle("cli.connections.list", () => [...connections])
    .handle("cli.consents.answer", (input) => {
      answers.push(input);
      consents = [];
    })
    .handle("cli.connections.disconnect", () => {
      connections = [];
    });
  const store = new CliAccessStore(createIpcClient(contract, bridge), events);
  return { bridge, events, store, answers };
}

async function settle() {
  for (let round = 0; round < 5; round += 1) await Promise.resolve();
}

test("a held change is answered and leaves the queue", async () => {
  const { store, answers } = rig();
  await store.refresh();
  expect(store.current?.id).toBe(CONSENT);
  await store.answer(false);
  expect(answers).toEqual([{ id: CONSENT, confirm: false }]);
  expect(store.current).toBeNull();
});

test("a cli step refreshes held changes and connections", async () => {
  const { store, events, bridge } = rig();
  store.start();
  await settle();
  const before = bridge.calls.length;
  events.dispatch({
    type: "step",
    streamId: STREAM,
    seq: 0,
    ts: 1,
    step: { domain: CLI_STEP_DOMAIN },
  });
  await settle();
  expect(bridge.calls.length).toBe(before + 2);
  store.dispose();
});

test("a failed answer is reported and a failed disconnect keeps the list", async () => {
  const bridge = createFakeBridge<Contract>()
    .handle("cli.consents.list", () => [consent()])
    .handle("cli.connections.list", () => [connection()])
    .fail("cli.consents.answer", AppErrorCode.NOT_FOUND, "x")
    .fail("cli.connections.disconnect", AppErrorCode.NOT_FOUND, "x");
  const store = new CliAccessStore(createIpcClient(contract, bridge), createEventRouter());
  await store.refresh();
  await store.answer(true);
  expect(store.consentError).toBe(CLI_CONSENT_FAILED);
  expect(store.answering).toBe(false);
  await store.disconnect(1);
  expect(store.connections).toHaveLength(1);
});

test("disconnecting refreshes the list", async () => {
  const { store } = rig();
  await store.refresh();
  await store.disconnect(1);
  expect(store.connections).toEqual([]);
  expect(store.error).not.toBe(CLI_DISCONNECT_FAILED);
});

test("the change and its source are named", () => {
  expect(cliChangeText(consent().change)).toContain(EXEC_MODE_COPY["no-restrictions"].label);
  expect(cliChangeText({ kind: "allowRule", scope: "anywhere" })).toBe(CLI_CONSENT_RULE_ANYWHERE);
  expect(cliSourceText(consent())).toBe(CLI_CONSENT_SOURCE_RECOGNISED);
  const unknown = cliSourceText(consent({ recognised: false, image: "C:/x/tool.exe" }));
  expect(unknown.startsWith(CLI_CONSENT_SOURCE_UNKNOWN)).toBe(true);
  expect(unknown).toContain("C:/x/tool.exe");
  expect(cliClientLine(connection()).startsWith(CLI_CLIENT_RECOGNISED)).toBe(true);
  expect(
    cliClientLine(connection({ recognised: false, pid: null })).startsWith(CLI_CLIENT_UNKNOWN),
  ).toBe(true);
});
