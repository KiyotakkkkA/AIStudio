import { expect, test } from "vitest";
import { ChatToolCallDto } from "@zvs/shared";
import {
  answerBlocks,
  appendRound,
  currentRound,
} from "../../src/renderer/features/chat/answerRounds.ts";

function call(id: string, round: number): ChatToolCallDto {
  return ChatToolCallDto.parse({
    id,
    key: "demo.lookup",
    name: "demo__lookup",
    round,
    arguments: {},
    status: "succeeded",
  });
}

test("each round shows its reasoning and text before the calls it made", () => {
  const calls = [call("a", 0), call("b", 0), call("c", 1)];
  const rounds = [
    { text: "Поиск.", reasoning: "Первая мысль." },
    { text: "\n\nУточнение.", reasoning: "Вторая мысль." },
    { text: "\n\nИтог.", reasoning: "" },
  ];
  const blocks = answerBlocks(
    "Поиск.\n\nУточнение.\n\nИтог.",
    "Первая мысль.Вторая мысль.",
    rounds,
    calls,
  );
  expect(blocks).toEqual([
    { kind: "text", key: "text-0", text: "Поиск.", reasoning: "Первая мысль." },
    { kind: "tools", key: "tools-0", calls: [calls[0], calls[1]] },
    { kind: "text", key: "text-1", text: "\n\nУточнение.", reasoning: "Вторая мысль." },
    { kind: "tools", key: "tools-1", calls: [calls[2]] },
    { kind: "text", key: "text-2", text: "\n\nИтог.", reasoning: "" },
  ]);
});

test("text past the saved rounds belongs to the round after the last calls", () => {
  const calls = [call("a", 0)];
  const blocks = answerBlocks(
    "Поиск.\n\nЧастичный",
    "Мысль.Ещё",
    [{ text: "Поиск.", reasoning: "Мысль." }],
    calls,
  );
  expect(blocks.map((block) => block.kind)).toEqual(["text", "tools", "text"]);
  expect(blocks[2]).toEqual({
    kind: "text",
    key: "text-1",
    text: "\n\nЧастичный",
    reasoning: "Ещё",
  });
});

test("a message without rounds keeps reasoning first and the answer after the calls", () => {
  const calls = [call("a", 0)];
  expect(answerBlocks("Ответ", "Мысль", [], calls)).toEqual([
    { kind: "text", key: "reasoning", text: "", reasoning: "Мысль" },
    { kind: "tools", key: "tools", calls },
    { kind: "text", key: "answer", text: "Ответ", reasoning: "" },
  ]);
  expect(answerBlocks("Ответ", "", [{ text: "Другое", reasoning: "" }], [])).toEqual([
    { kind: "text", key: "answer", text: "Ответ", reasoning: "" },
  ]);
});

test("streamed deltas go to the round after the latest call", () => {
  expect(currentRound([])).toBe(0);
  let rounds = appendRound([], [], "reasoning", "Мысль");
  rounds = appendRound(rounds, [], "text", "Поиск.");
  const calls = [call("a", 0)];
  rounds = appendRound(rounds, calls, "text", "\n\nИтог");
  rounds = appendRound(rounds, calls, "text", ".");
  expect(rounds).toEqual([
    { text: "Поиск.", reasoning: "Мысль" },
    { text: "\n\nИтог.", reasoning: "" },
  ]);
  expect(appendRound([], [call("a", 2)], "text", "x")).toEqual([
    { text: "", reasoning: "" },
    { text: "", reasoning: "" },
    { text: "", reasoning: "" },
    { text: "x", reasoning: "" },
  ]);
});
