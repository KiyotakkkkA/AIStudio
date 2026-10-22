import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  ConversationDetailDto,
  RunHandleDto,
  contract,
  type ChatToolCallDto,
  type Contract,
  type ProcessPlanDto,
  type ProcessResultDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import {
  CODE_LEAD,
  NO_EXIT_CODE,
  SHELL_LEAD,
  TIMED_OUT,
  commandLine,
  durationLabel,
  exitLabel,
  processLead,
  processResult,
  processSummary,
} from "../../src/renderer/features/chat/processCopy.ts";
import { toolCallSummary } from "../../src/renderer/features/chat/toolCallSummary.ts";
import { provider } from "./providerFixtures.ts";

const saved = provider();
const handle = RunHandleDto.parse({
  id: "0199ee11-7777-7111-8111-000000000002",
  streamId: "0199ee11-7777-7111-8111-000000000003",
});
const conversation = ConversationDetailDto.parse({
  id: "0199ee11-7777-7111-8111-000000000001",
  title: "Checks",
  providerId: saved.id,
  modelId: saved.models[0]!.externalId,
  createdAt: 1,
  updatedAt: 1,
  messages: [],
});

const command: ProcessPlanDto = {
  program: "C:\\Program Files\\nodejs\\npm.cmd",
  argv: ["npm", "run", "test unit"],
  cwd: "D:\\work\\app",
  packages: [],
};
const script: ProcessPlanDto = {
  program: "uv",
  language: "python",
  argv: [],
  cwd: "D:\\scratch\\run",
  source: "\n# /// script\nprint(1)\n",
  packages: [],
};

function result(overrides: Partial<ProcessResultDto>): ProcessResultDto {
  return {
    process: command,
    exitCode: 0,
    durationMs: 1250,
    timedOut: false,
    truncated: false,
    ...overrides,
  };
}

test("the tool meta yields a process result only when it matches the contract", () => {
  expect(processResult(result({}))).toEqual(result({}));
  expect(processResult({ exitCode: 0 })).toBeNull();
  expect(processResult(null)).toBeNull();
});

test("labels quote arguments and name the exit and the duration", () => {
  expect(commandLine(command.argv)).toBe('npm run "test unit"');
  expect(commandLine(["echo", 'say "hi"', ""])).toBe('echo "say \\"hi\\"" ""');
  expect(exitLabel(result({ exitCode: 2 }))).toBe("Код завершения: 2");
  expect(exitLabel(result({ exitCode: null, timedOut: true }))).toBe(TIMED_OUT);
  expect(exitLabel(result({ exitCode: null }))).toBe(NO_EXIT_CODE);
  expect(durationLabel(640)).toBe("640 мс");
  expect(durationLabel(1250)).toBe(`${(1.3).toLocaleString("ru-RU")} с`);
  expect(processLead(command)).toBe(SHELL_LEAD);
  expect(processLead(script)).toBe(CODE_LEAD);
  expect(processSummary(script)).toBe("Python · # /// script");
});

test("the call summary shows the command before and after the run", () => {
  const running = {
    key: "shell.run",
    arguments: { argv: ["cargo", "check"] },
    meta: null,
  } as unknown as ChatToolCallDto;
  expect(toolCallSummary(running)).toBe("cargo check");
  const done = {
    key: "shell.run",
    arguments: { argv: ["npm"] },
    meta: result({}),
  } as unknown as ChatToolCallDto;
  expect(toolCallSummary(done)).toBe('npm run "test unit"');
  const code = {
    key: "code.run",
    arguments: { language: "powershell", source: "Get-Date" },
    meta: null,
  } as unknown as ChatToolCallDto;
  expect(toolCallSummary(code)).toBe("PowerShell");
});

test("a process approval keeps its plan for the tool card", async () => {
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [conversation])
    .handle("chat.conversations.get", () => conversation)
    .handle("providers.list", () => [saved])
    .handle("providers.get", () => saved)
    .handle("vectorStores.list", () => [])
    .handle("chat.send", () => handle)
    .handle("runs.approve", () => undefined);
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new ChatStore(createIpcClient(contract, bridge), events);
  await store.mount();
  await store.select(conversation.id);
  store.composer.setText("Run the tests");
  await store.send();
  const request = {
    id: "0199ee11-7777-7111-8111-000000000009",
    runId: handle.id,
    kind: "process",
    subject: "shell.run",
    scope: "global",
    expiresAt: 60_000,
    conversationId: conversation.id,
    tool: "shell.run",
    action: "запуск команды",
    process: command,
  };
  events.dispatch({ type: "approval", ...handle, seq: 1, ts: 1, request });
  expect(store.folderRequest).toBeNull();
  expect(store.approvalFor(request.id)?.process).toEqual(command);
  expect(store.approvalFor(request.id)?.changes).toEqual([]);
  await store.decideApproval(request.id, true);
  expect(bridge.calls.at(-1)).toEqual({
    channel: "runs.approve",
    payload: { id: handle.id, approvalId: request.id, always: false },
  });
  store.dispose();
});
