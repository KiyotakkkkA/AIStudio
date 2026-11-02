import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  contract,
  type ChatContextInput,
  type ChatStatsDto,
  type Contract,
  MessageDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import ChatStatsStore from "../../src/renderer/features/chat/stats/ChatStatsStore.ts";
import {
  carriedRows,
  chatHints,
  dialogTotals,
  repeatedCalls,
  sectionRows,
  toolCallCounts,
  turnRows,
} from "../../src/renderer/features/chat/stats/chatStatsModel.ts";
import {
  CARRIED_KIND_LABELS,
  CARRIED_STATUS_LABELS,
  CARRIED_TRUNCATED,
  REASONING_LABEL,
} from "../../src/renderer/features/chat/stats/chatStatsCopy.ts";

const CONVERSATION = "0199aa11-2222-7111-8111-111111111111";
const PROVIDER = "0199aa11-1111-7111-8111-111111111111";

let sequence = 0;

function message(role: "user" | "assistant", patch: Partial<MessageDto> = {}): MessageDto {
  sequence += 1;
  return MessageDto.parse({
    id: `0199aa11-3333-7111-8111-${String(sequence).padStart(12, "0")}`,
    conversationId: CONVERSATION,
    role,
    content: role === "user" ? "Вопрос" : "Ответ",
    citations: [],
    tokensIn: 0,
    tokensOut: 0,
    usageEstimated: true,
    durationMs: 0,
    runId: null,
    partial: false,
    createdAt: sequence,
    ...patch,
  });
}

function call(key: string, args: Record<string, string>) {
  return {
    id: `call-${key}-${JSON.stringify(args)}`,
    key,
    name: key,
    round: 0,
    arguments: args,
    status: "succeeded" as const,
    result: null,
    truncated: false,
    error: null,
    approvalId: null,
    stepId: null,
    durationMs: 0,
    meta: null,
  };
}

function stats(patch: Partial<ChatStatsDto["context"]> = {}): ChatStatsDto {
  return {
    context: {
      contextWindow: 10_000,
      reservedOutput: 1_000,
      system: 500,
      tools: 4_000,
      toolItems: [
        { key: "fs.read", tokens: 1_500, source: "base" },
        { key: "mail.send", tokens: 2_500, source: "base" },
      ],
      history: 300,
      historyMessages: 2,
      trimmed: 0,
      total: 4_800,
      ...patch,
    },
    tokenizer: "o200k_base",
    sections: [
      { key: "dialogPrompt", tokens: 100, text: "Промпт" },
      { key: "memory", tokens: 2_000, text: "Память" },
      { key: "goal", tokens: 0, text: "" },
    ],
    deferred: [],
    history: [],
  };
}

function dialog(): MessageDto[] {
  return [
    message("user"),
    message("assistant", {
      tokensIn: 3_000,
      tokensOut: 200,
      tokensCached: 500,
      tokensReasoning: 50,
      usageEstimated: false,
      toolCalls: [call("fs.read", { path: "a.md" })],
    }),
    message("user"),
    message("assistant", {
      tokensIn: 4_000,
      tokensOut: 100,
      toolCalls: [call("fs.read", { path: "a.md" })],
    }),
    message("user"),
    message("assistant", { tokensIn: 5_000, tokensOut: 80, partial: true }),
    message("assistant", { tokensIn: 4_500, tokensOut: 60 }),
  ];
}

test("turn rows cover finished answers and the totals add input and output separately", () => {
  const rows = turnRows(dialog());
  expect(rows.map((row) => row.index)).toEqual([1, 2, 3]);
  expect(rows[0]).toMatchObject({ input: 3_000, cached: 500, reasoning: 50, estimated: false });
  expect(dialogTotals(rows)).toEqual({ input: 11_500, output: 360, cached: 500, reasoning: 50 });
});

test("calls are counted per tool and identical calls in two answers are reported once", () => {
  const messages = dialog();
  expect(toolCallCounts(messages).get("fs.read")).toBe(2);
  expect(repeatedCalls(messages)).toEqual([{ key: "fs.read", answers: 2 }]);
});

test("sections are sorted by size and the hint list ranks savings first", () => {
  expect(sectionRows(stats()).map((row) => row.key)).toEqual(["memory", "dialogPrompt", "goal"]);
  const hints = chatHints({
    stats: stats(),
    messages: dialog(),
    groups: [],
    toolLabel: (key) => key,
  });
  expect(hints.map((hint) => hint.key)).toEqual([
    "tools",
    "section:memory",
    "repeat:fs.read",
    "cache",
  ]);
  expect(hints[0]?.saving).toBe(2_500);
});

test("usage hints wait for three answers while structural hints apply at once", () => {
  const hints = chatHints({
    stats: stats({ reservedOutput: 4_000 }),
    messages: [message("user")],
    groups: [{ key: "mcp:x", label: "X", mcp: true, keys: ["mail.send"] }],
    toolLabel: (key) => key,
  });
  expect(hints.map((hint) => hint.key)).toEqual(["section:memory", "reserve"]);
  expect(hints[1]?.saving).toBe(1_500);
});

test("an MCP server without calls is named when it takes more than its share", () => {
  const hints = chatHints({
    stats: stats(),
    messages: dialog(),
    groups: [{ key: "mcp:x", label: "Почта", mcp: true, keys: ["mail.send"] }],
    toolLabel: (key) => key,
  });
  expect(hints.some((hint) => hint.key === "server:mcp:x")).toBe(true);
});

test("trimmed history is a hint", () => {
  const trimmed = {
    ...stats({ tools: 100 }),
    history: [
      {
        messageId: message("user").id,
        role: "user" as const,
        status: "trimmed" as const,
        tokens: 900,
        text: "Старое",
        reasoningTokens: 0,
        carried: [],
      },
    ],
  };
  const hints = chatHints({ stats: trimmed, messages: [], groups: [], toolLabel: (key) => key });
  expect(hints.find((hint) => hint.key === "trimmed")?.saving).toBe(900);
});

test("carried parts map their codes to copy and reasoning is listed as not sent", () => {
  const entry = {
    messageId: message("assistant").id,
    role: "assistant" as const,
    status: "included" as const,
    tokens: 30,
    text: "Ответ [1]",
    reasoningTokens: 12,
    carried: [
      {
        kind: "toolCall" as const,
        label: "fs.read",
        marker: "",
        status: "sent" as const,
        tokens: 2010,
        truncated: true,
      },
      {
        kind: "toolCall" as const,
        label: "fs.search",
        marker: "",
        status: "shortened" as const,
        tokens: 40,
        truncated: false,
      },
      {
        kind: "source" as const,
        label: "policy.md",
        marker: "[1]",
        status: "dropped" as const,
        tokens: 90,
        truncated: false,
      },
    ],
  };
  const rows = carriedRows(entry, (key) => `tool:${key}`);
  expect(rows.map((row) => [row.kind, row.label, row.statusLabel, row.note, row.tokens])).toEqual([
    [
      CARRIED_KIND_LABELS.toolCall,
      "tool:fs.read",
      CARRIED_STATUS_LABELS.sent,
      CARRIED_TRUNCATED,
      2010,
    ],
    [CARRIED_KIND_LABELS.toolCall, "tool:fs.search", CARRIED_STATUS_LABELS.shortened, "", 40],
    [CARRIED_KIND_LABELS.source, "[1] policy.md", CARRIED_STATUS_LABELS.dropped, "", 90],
    [REASONING_LABEL, "", CARRIED_STATUS_LABELS.dropped, "", 12],
  ]);
  expect(carriedRows({ ...entry, carried: [], reasoningTokens: 0 }, (key) => key)).toEqual([]);
});

const INPUT: ChatContextInput = {
  conversationId: CONVERSATION as ChatContextInput["conversationId"],
  providerId: PROVIDER as ChatContextInput["providerId"],
  modelId: "model",
};

test("statistics load only while open and the latest request wins", async () => {
  const bridge = createFakeBridge<Contract>().handle("chat.stats", () => stats());
  const store = new ChatStatsStore(createIpcClient(contract, bridge));

  await store.refresh(INPUT);
  expect(bridge.calls).toHaveLength(0);

  store.toggle();
  await Promise.resolve();
  await store.load();
  expect(store.open).toBe(true);
  expect(store.stats?.tokenizer).toBe("o200k_base");
  expect(bridge.calls.at(-1)?.payload).toEqual(INPUT);

  await store.refresh(null);
  expect(store.stats).toBeNull();

  store.close();
  expect(store.open).toBe(false);
});

test("a failed load is reported and can be retried", async () => {
  const store = new ChatStatsStore(
    createIpcClient(
      contract,
      createFakeBridge<Contract>().fail("chat.stats", AppErrorCode.UNKNOWN, "x"),
    ),
  );
  await store.refresh(INPUT);
  store.toggle();
  await store.load();
  expect(store.failed).toBe(true);
  expect(store.loading).toBe(false);
});
