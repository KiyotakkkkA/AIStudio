import type { ChatRoundDto, ChatToolCallDto } from "@zvs/shared";

export type AnswerBlock =
  | {
      readonly kind: "text";
      readonly key: string;
      readonly text: string;
      readonly reasoning: string;
    }
  | { readonly kind: "tools"; readonly key: string; readonly calls: readonly ChatToolCallDto[] };

export function currentRound(calls: readonly ChatToolCallDto[]): number {
  return calls.reduce((next, call) => Math.max(next, call.round + 1), 0);
}

type RoundText = "text" | "reasoning";

const EMPTY_ROUND: ChatRoundDto = { text: "", reasoning: "", estimated: false };

export function appendRound(
  rounds: readonly ChatRoundDto[],
  calls: readonly ChatToolCallDto[],
  field: RoundText,
  delta: string,
): ChatRoundDto[] {
  const index = currentRound(calls);
  const next = [...rounds];
  while (next.length <= index) next.push(EMPTY_ROUND);
  const round = next[index] ?? EMPTY_ROUND;
  next[index] = { ...round, [field]: round[field] + delta };
  return next;
}

function joined(rounds: readonly ChatRoundDto[], field: RoundText): string {
  return rounds.map((round) => round[field]).join("");
}

function textBlock(key: string, text: string, reasoning: string): AnswerBlock[] {
  return text.trim().length === 0 && reasoning.trim().length === 0
    ? []
    : [{ kind: "text", key, text, reasoning }];
}

function toolsBlock(key: string, calls: readonly ChatToolCallDto[]): AnswerBlock[] {
  return calls.length === 0 ? [] : [{ kind: "tools", key, calls }];
}

export function answerBlocks(
  content: string,
  reasoning: string,
  rounds: readonly ChatRoundDto[],
  calls: readonly ChatToolCallDto[],
): AnswerBlock[] {
  const text = joined(rounds, "text");
  const thought = joined(rounds, "reasoning");
  if (rounds.length === 0 || !content.startsWith(text) || !reasoning.startsWith(thought)) {
    return [
      ...textBlock("reasoning", "", reasoning),
      ...toolsBlock("tools", calls),
      ...textBlock("answer", content, ""),
    ];
  }
  const all = [
    ...rounds,
    {
      ...EMPTY_ROUND,
      text: content.slice(text.length),
      reasoning: reasoning.slice(thought.length),
    },
  ];
  return all.flatMap((round, index) => [
    ...textBlock(`text-${String(index)}`, round.text, round.reasoning),
    ...toolsBlock(
      `tools-${String(index)}`,
      calls.filter((call) =>
        index === all.length - 1 ? call.round >= index : call.round === index,
      ),
    ),
  ]);
}
