import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  ConversationDetailDto,
  RunHandleDto,
  contract,
  type ChatToolCallDto,
  type Contract,
  type FileDiffDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import {
  changesSummary,
  diffLineTone,
  diffLines,
  fileChanges,
  moreChanges,
} from "../../src/renderer/features/chat/fileChangesCopy.ts";
import { toolCallSummary } from "../../src/renderer/features/chat/toolCallSummary.ts";
import { provider } from "./providerFixtures.ts";

const saved = provider();
const handle = RunHandleDto.parse({
  id: "0199ee11-6666-7111-8111-000000000002",
  streamId: "0199ee11-6666-7111-8111-000000000003",
});
const conversation = ConversationDetailDto.parse({
  id: "0199ee11-6666-7111-8111-000000000001",
  title: "Refactor",
  providerId: saved.id,
  modelId: saved.models[0]!.externalId,
  createdAt: 1,
  updatedAt: 1,
  messages: [],
});

const edited: FileDiffDto = {
  path: "D:\\work\\a.ts",
  kind: "modify",
  added: 2,
  removed: 1,
  diff: "@@ -1,2 +1,3 @@\n context\n-old\n+new\n+more\n",
  truncated: false,
};
const moved: FileDiffDto = {
  path: "D:\\work\\b.ts",
  from: "D:\\work\\old.ts",
  kind: "move",
  added: 0,
  removed: 0,
  diff: "",
  truncated: false,
};

test("tool meta yields the changes only when it matches the contract", () => {
  expect(fileChanges({ changes: [edited, moved] })).toEqual([edited, moved]);
  expect(fileChanges({ changes: [{ ...moved, from: undefined }] })).toEqual([]);
  expect(fileChanges(null)).toEqual([]);
});

test("the summary counts files and lines and the diff lines carry their tone", () => {
  expect(changesSummary([edited, moved])).toBe("2 файла · +2 −1");
  expect(moreChanges(5)).toBe("Показать ещё 5 файлов");
  expect(diffLines(edited.diff).map(diffLineTone)).toEqual([
    "hunk",
    "context",
    "removed",
    "added",
    "added",
  ]);
  const call = {
    key: "fs.apply",
    arguments: { changes: [] },
    meta: { changes: [edited] },
  } as unknown as ChatToolCallDto;
  expect(toolCallSummary(call)).toBe("1 файл · +2 −1");
});

test("a files approval keeps its diff for the tool card", async () => {
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
  store.composer.setText("Rename it");
  await store.send();
  const request = {
    id: "0199ee11-6666-7111-8111-000000000009",
    runId: handle.id,
    kind: "files",
    subject: "fs.apply",
    scope: "global",
    expiresAt: 60_000,
    conversationId: conversation.id,
    tool: "fs.apply",
    action: "изменение файлов",
    changes: [edited, moved],
  };
  events.dispatch({ type: "approval", ...handle, seq: 1, ts: 1, request });
  expect(store.folderRequest).toBeNull();
  expect(store.approvalFor(request.id)?.changes).toEqual([edited, moved]);
  await store.decideApproval(request.id, true);
  expect(bridge.calls.at(-1)).toEqual({
    channel: "runs.approve",
    payload: { id: handle.id, approvalId: request.id, always: false },
  });
  store.dispose();
});
