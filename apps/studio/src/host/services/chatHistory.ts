import { AppError, AppErrorCode, type ChatCitationDto, type VectorSearchHitDto } from "@zvs/shared";
import type { ChatMessage } from "../drivers/ai/ports.ts";

export const estimateTokens = (text: string): number => Buffer.byteLength(text, "utf8");
const messageCost = (message: ChatMessage): number => estimateTokens(message.content) + 8;
export function windowChatHistory(
  system: string,
  history: readonly ChatMessage[],
  current: ChatMessage,
  contextWindow: number,
  maxOutputTokens: number,
  sources: readonly { storeId: ChatCitationDto["storeId"]; hit: VectorSearchHitDto }[] = [],
) {
  const budget = contextWindow - maxOutputTokens;
  let used = estimateTokens(system) + 8 + messageCost(current);
  if (used > budget)
    throw new AppError(
      AppErrorCode.VALIDATION_FAILED,
      "System prompt and current turn exceed the model context budget",
    );
  const citations: ChatCitationDto[] = [];
  const passages: string[] = [];
  for (const { storeId, hit } of sources) {
    const payload = hit.payload;
    const text =
      typeof payload === "string"
        ? payload
        : payload !== null &&
            !Array.isArray(payload) &&
            typeof payload === "object" &&
            typeof payload.text === "string"
          ? payload.text
          : "";
    if (!text) continue;
    const passage = `[${citations.length + 1}] ${hit.path}\n${text}`;
    const cost =
      estimateTokens(passage + "\n\n") +
      (passages.length
        ? 0
        : messageCost({ role: "user", content: "Retrieved sources (reference material):\n" }));
    if (used + cost > budget) continue;
    used += cost;
    passages.push(passage);
    citations.push({
      storeId,
      documentId: hit.documentId,
      sourcePath: hit.path,
      chunkIndex: hit.chunkIndex,
      score: hit.score,
    });
  }
  const recent: ChatMessage[] = [];
  for (let index = history.length - 1; index >= 0; index--) {
    const item = history[index]!;
    if (used + messageCost(item) > budget) break;
    used += messageCost(item);
    recent.unshift(item);
  }
  while (recent[0]?.role === "assistant") used -= messageCost(recent.shift()!);
  const messages: ChatMessage[] = [...recent];
  if (passages.length)
    messages.push({
      role: "user",
      content: `Retrieved sources (reference material):\n${passages.join("\n\n")}`,
    });
  messages.push(current);
  return { messages, citations, tokensIn: used, trimmed: history.length - recent.length };
}
