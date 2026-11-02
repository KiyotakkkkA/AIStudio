import type {
  ChatCarriedStatus,
  ChatHistoryEntryDto,
  ChatPromptSection,
  ChatStatsDto,
  MessageDto,
} from "@zvs/shared";
import {
  CARRIED_KIND_LABELS,
  CARRIED_STATUS_LABELS,
  CARRIED_TRUNCATED,
  REASONING_LABEL,
  SECTION_LABELS,
  answersWord,
  messagesWord,
  tokensWord,
  toolsWord,
} from "./chatStatsCopy";

export const TOOLS_SHARE_LIMIT = 0.3;
export const SERVER_SHARE_LIMIT = 0.15;
export const SECTION_SHARE_LIMIT = 0.15;
export const RESERVE_SHARE_LIMIT = 0.25;
export const CACHE_SHARE_FLOOR = 0.3;
export const ANSWERS_FOR_USAGE_HINTS = 3;
export const RECENT_TURNS = 5;
export const HINTS_MAX = 5;

export interface TurnRow {
  readonly id: string;
  readonly index: number;
  readonly input: number;
  readonly cached: number;
  readonly output: number;
  readonly reasoning: number;
  readonly estimated: boolean;
  readonly durationMs: number;
  readonly rounds: number;
  readonly toolCalls: number;
}

export interface DialogTotals {
  readonly input: number;
  readonly output: number;
  readonly cached: number;
  readonly reasoning: number;
}

export interface ToolGroup {
  readonly key: string;
  readonly label: string;
  readonly mcp: boolean;
  readonly keys: readonly string[];
}

export interface Hint {
  readonly key: string;
  readonly title: string;
  readonly detail: string;
  readonly saving: number;
}

export interface SectionRow {
  readonly key: ChatPromptSection;
  readonly label: string;
  readonly tokens: number;
  readonly text: string;
}

function answers(messages: readonly MessageDto[]): MessageDto[] {
  return messages.filter((message) => message.role === "assistant" && !message.partial);
}

export function turnRows(messages: readonly MessageDto[]): TurnRow[] {
  return answers(messages).map((message, index) => ({
    id: message.id,
    index: index + 1,
    input: message.tokensIn,
    cached: Math.min(message.tokensCached, message.tokensIn),
    output: message.tokensOut,
    reasoning: message.tokensReasoning,
    estimated: message.usageEstimated,
    durationMs: message.durationMs,
    rounds: Math.max(1, message.rounds.length),
    toolCalls: message.toolCalls.length,
  }));
}

export function dialogTotals(rows: readonly TurnRow[]): DialogTotals {
  return rows.reduce<DialogTotals>(
    (sum, row) => ({
      input: sum.input + row.input,
      output: sum.output + row.output,
      cached: sum.cached + row.cached,
      reasoning: sum.reasoning + row.reasoning,
    }),
    { input: 0, output: 0, cached: 0, reasoning: 0 },
  );
}

export function sectionRows(stats: ChatStatsDto): SectionRow[] {
  return stats.sections
    .map((section) => ({
      key: section.key,
      label: SECTION_LABELS[section.key],
      tokens: section.tokens,
      text: section.text,
    }))
    .sort((left, right) => right.tokens - left.tokens);
}

export function toolCallCounts(messages: readonly MessageDto[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const message of messages)
    for (const call of message.toolCalls) counts.set(call.key, (counts.get(call.key) ?? 0) + 1);
  return counts;
}

export interface RepeatedCall {
  readonly key: string;
  readonly answers: number;
}

function stable(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stable).join(",")}]`;
  if (value !== null && typeof value === "object")
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${stable((value as Record<string, unknown>)[key])}`)
      .join(",")}}`;
  return JSON.stringify(value);
}

export function repeatedCalls(messages: readonly MessageDto[]): RepeatedCall[] {
  const seen = new Map<string, { key: string; answers: Set<string> }>();
  for (const message of answers(messages))
    for (const call of message.toolCalls) {
      const signature = `${call.key}\u0000${stable(call.arguments)}`;
      const entry = seen.get(signature) ?? { key: call.key, answers: new Set<string>() };
      entry.answers.add(message.id);
      seen.set(signature, entry);
    }
  return [...seen.values()]
    .filter((entry) => entry.answers.size > 1)
    .map((entry) => ({ key: entry.key, answers: entry.answers.size }))
    .sort((left, right) => right.answers - left.answers);
}

function share(value: number, total: number): number {
  return total > 0 ? value / total : 0;
}

function percentText(value: number, total: number): string {
  return `${Math.round(share(value, total) * 100).toLocaleString("ru-RU")} %`;
}

export function chatHints({
  stats,
  messages,
  groups,
  toolLabel,
}: {
  readonly stats: ChatStatsDto;
  readonly messages: readonly MessageDto[];
  readonly groups: readonly ToolGroup[];
  readonly toolLabel: (key: string) => string;
}): Hint[] {
  const { context } = stats;
  const window = context.contextWindow;
  const calls = toolCallCounts(messages);
  const answered = answers(messages).length;
  const hints: Hint[] = [];

  if (answered >= ANSWERS_FOR_USAGE_HINTS && share(context.tools, window) > TOOLS_SHARE_LIMIT) {
    const unused = context.toolItems.filter((item) => !calls.has(item.key));
    const saving = unused.reduce((sum, item) => sum + item.tokens, 0);
    if (unused.length > 0)
      hints.push({
        key: "tools",
        title: `Инструменты занимают ${percentText(context.tools, window)} окна`,
        detail: `В диалоге не вызывались ${toolsWord(unused.length)}, около ${tokensWord(saving)}. Отключение неиспользуемых инструментов освобождает это место.`,
        saving,
      });
  }

  if (answered >= ANSWERS_FOR_USAGE_HINTS)
    for (const group of groups) {
      if (!group.mcp) continue;
      const items = context.toolItems.filter((item) => group.keys.includes(item.key));
      const tokens = items.reduce((sum, item) => sum + item.tokens, 0);
      if (share(tokens, window) <= SERVER_SHARE_LIMIT) continue;
      if (items.some((item) => calls.has(item.key))) continue;
      hints.push({
        key: `server:${group.key}`,
        title: `MCP-сервер «${group.label}» занимает ${percentText(tokens, window)} окна`,
        detail: `Ни один из его инструментов не вызывался в диалоге. Отключение сервера для этого диалога освобождает ${tokensWord(tokens)}.`,
        saving: tokens,
      });
    }

  const trimmed = stats.history.filter((entry) => entry.status === "trimmed");
  if (trimmed.length > 0) {
    const tokens = trimmed.reduce((sum, entry) => sum + entry.tokens, 0);
    hints.push({
      key: "trimmed",
      title: `${messagesWord(trimmed.length)} не помещаются в окно`,
      detail: `Ранние сообщения, около ${tokensWord(tokens)}, не попадут в следующий запрос. Важное стоит закрепить или продолжить в новом диалоге.`,
      saving: tokens,
    });
  }

  for (const section of stats.sections) {
    if (section.key === "toolSpecs") continue;
    if (share(section.tokens, window) <= SECTION_SHARE_LIMIT) continue;
    hints.push({
      key: `section:${section.key}`,
      title: `Раздел «${SECTION_LABELS[section.key]}» занимает ${percentText(section.tokens, window)} окна`,
      detail: `Это ${tokensWord(section.tokens)} в каждом запросе. Сокращение раздела уменьшает вход каждого ответа.`,
      saving: section.tokens,
    });
  }

  if (share(context.reservedOutput, window) > RESERVE_SHARE_LIMIT) {
    const saving = Math.round(context.reservedOutput - window * RESERVE_SHARE_LIMIT);
    hints.push({
      key: "reserve",
      title: `Резерв под ответ — ${percentText(context.reservedOutput, window)} окна`,
      detail: `Резерв вычитается из места под историю. Меньший лимит ответа в настройках модели освобождает до ${tokensWord(saving)}.`,
      saving,
    });
  }

  for (const repeated of repeatedCalls(messages))
    hints.push({
      key: `repeat:${repeated.key}`,
      title: `${toolLabel(repeated.key)}: одинаковые вызовы в ${answersWord(repeated.answers)}`,
      detail:
        "Результаты прошлых вызовов передаются модели, пока помещаются в окно. При нехватке места они сокращаются, и вызов повторяется. Короткий вывод из результата в сообщении сохраняет его в истории.",
      saving: 0,
    });

  const recent = turnRows(messages).slice(-RECENT_TURNS);
  const reported = recent.filter((row) => !row.estimated);
  if (reported.some((row) => row.cached > 0)) {
    const input = reported.reduce((sum, row) => sum + row.input, 0);
    const cached = reported.reduce((sum, row) => sum + row.cached, 0);
    if (share(cached, input) < CACHE_SHARE_FLOOR)
      hints.push({
        key: "cache",
        title: `Из кэша — ${percentText(cached, input)} входа`,
        detail: `За последние ${answersWord(reported.length)} начало запроса менялось от хода к ходу. Неизменные промпт диалога и набор инструментов повышают долю кэша.`,
        saving: 0,
      });
  }

  return hints.sort((left, right) => right.saving - left.saving).slice(0, HINTS_MAX);
}

export interface CarriedRow {
  readonly key: string;
  readonly kind: string;
  readonly label: string;
  readonly status: ChatCarriedStatus;
  readonly statusLabel: string;
  readonly note: string;
  readonly tokens: number;
}

export function carriedRows(
  entry: ChatHistoryEntryDto,
  toolLabel: (key: string) => string,
): CarriedRow[] {
  const rows: CarriedRow[] = entry.carried.map((part, index) => ({
    key: `${part.kind}:${String(index)}`,
    kind: CARRIED_KIND_LABELS[part.kind],
    label: part.kind === "toolCall" ? toolLabel(part.label) : `${part.marker} ${part.label}`.trim(),
    status: part.status,
    statusLabel: CARRIED_STATUS_LABELS[part.status],
    note: part.truncated ? CARRIED_TRUNCATED : "",
    tokens: part.tokens,
  }));
  if (entry.reasoningTokens > 0)
    rows.push({
      key: "reasoning",
      kind: REASONING_LABEL,
      label: "",
      status: "dropped",
      statusLabel: CARRIED_STATUS_LABELS.dropped,
      note: "",
      tokens: entry.reasoningTokens,
    });
  return rows;
}
