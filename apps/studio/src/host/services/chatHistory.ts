import {
  AppError,
  AppErrorCode,
  type ChatCitationDto,
  type Json,
  type VectorChunkDto,
  type VectorSearchHitDto,
} from "@zvs/shared";
import type { ChatMessage } from "../drivers/ai/ports.ts";
import { hitText } from "./rerank.ts";

export const estimateTokens = (text: string): number => Buffer.byteLength(text, "utf8");
const messageCost = (message: ChatMessage): number => estimateTokens(message.content) + 8;

export const SOURCES_HEADING = "Retrieved sources (reference material):";
export const RELATED_HEADING = "Related documents (linked to the sources above, not ranked):";

export interface RetrievedSource {
  readonly storeId: ChatCitationDto["storeId"];
  readonly hit: VectorSearchHitDto;
  readonly neighbours?: readonly VectorChunkDto[];
}

export interface RelatedSource {
  readonly storeId: ChatCitationDto["storeId"];
  readonly hit: VectorSearchHitDto;
  readonly relation: string;
}

interface Piece {
  readonly index: number;
  readonly text: string;
  readonly payload: Json;
  readonly rank?: number;
}

interface Group {
  readonly storeId: ChatCitationDto["storeId"];
  readonly documentId: string;
  readonly pieces: Map<number, Piece>;
  readonly hits: Map<number, { hit: VectorSearchHitDto; rank: number }>;
}

export function sourceLabel(
  path: string,
  page: number | null | undefined,
  section: string | undefined,
): string {
  return [
    path,
    ...(page == null ? [] : [`стр. ${String(page)}`]),
    ...(section ? [section] : []),
  ].join(" · ");
}

function offset(payload: Json, key: "byteStart" | "byteEnd"): number | undefined {
  if (payload === null || typeof payload !== "object" || Array.isArray(payload)) return undefined;
  const value = payload[key];
  return typeof value === "number" && Number.isInteger(value) ? value : undefined;
}

export function joinChunks(pieces: readonly Pick<Piece, "text" | "payload">[]): string {
  let text = "";
  let end: number | undefined;
  for (const piece of pieces) {
    const start = offset(piece.payload, "byteStart");
    if (text === "") text = piece.text;
    else if (end !== undefined && start !== undefined && end > start) {
      const bytes = Buffer.from(piece.text, "utf8");
      const rest = bytes
        .subarray(Math.min(end - start, bytes.length))
        .toString("utf8")
        .trimStart();
      if (rest !== "") text += ` ${rest}`;
    } else text += `\n${piece.text}`;
    const pieceEnd = offset(piece.payload, "byteEnd");
    end = end === undefined || pieceEnd === undefined ? pieceEnd : Math.max(end, pieceEnd);
  }
  return text;
}

export function windowChatHistory(
  system: string,
  history: readonly ChatMessage[],
  current: ChatMessage,
  contextWindow: number,
  maxOutputTokens: number,
  sources: readonly RetrievedSource[] = [],
  related: readonly RelatedSource[] = [],
) {
  const budget = contextWindow - maxOutputTokens;
  let used = estimateTokens(system) + 8 + messageCost(current);
  if (used > budget)
    throw new AppError(
      AppErrorCode.VALIDATION_FAILED,
      "System prompt and current turn exceed the model context budget",
    );
  const blockCost = messageCost({ role: "user", content: `${SOURCES_HEADING}\n` });
  const groups = new Map<string, Group>();
  const order: Group[] = [];
  const selected: { source: RetrievedSource; group: Group; rank: number }[] = [];

  for (const source of sources) {
    const { storeId, hit } = source;
    const text = hitText(hit);
    if (!text) continue;
    const key = `${storeId}\u0000${hit.documentId}`;
    if (groups.get(key)?.pieces.has(hit.chunkIndex) === true) continue;
    const header = `[${String(selected.length + 1)}] ${sourceLabel(hit.path, hit.page, hit.section)}`;
    const cost = estimateTokens(`${header}\n${text}\n\n`) + (selected.length ? 0 : blockCost);
    if (used + cost > budget) continue;
    used += cost;
    let group = groups.get(key);
    if (!group) {
      group = { storeId, documentId: hit.documentId, pieces: new Map(), hits: new Map() };
      groups.set(key, group);
      order.push(group);
    }
    const rank = selected.length;
    group.pieces.set(hit.chunkIndex, { index: hit.chunkIndex, text, payload: hit.payload, rank });
    group.hits.set(hit.chunkIndex, { hit, rank });
    selected.push({ source, group, rank });
  }

  let neighbourCount = 0;
  for (const { source, group } of selected) {
    const nearest = [...(source.neighbours ?? [])]
      .filter((chunk) => Math.abs(chunk.chunkIndex - source.hit.chunkIndex) === 1)
      .sort((left, right) => left.chunkIndex - right.chunkIndex);
    for (const chunk of nearest) {
      if (group.pieces.has(chunk.chunkIndex) || chunk.text === "") continue;
      const cost = estimateTokens(`${chunk.text}\n`);
      if (used + cost > budget) continue;
      used += cost;
      neighbourCount += 1;
      group.pieces.set(chunk.chunkIndex, {
        index: chunk.chunkIndex,
        text: chunk.text,
        payload: chunk.payload,
      });
    }
  }

  const runs: { group: Group; pieces: Piece[]; rank: number }[] = [];
  for (const group of order) {
    const sorted = [...group.pieces.values()].sort((left, right) => left.index - right.index);
    let run: Piece[] = [];
    const close = () => {
      const ranks = run.flatMap((piece) => (piece.rank === undefined ? [] : [piece.rank]));
      if (ranks.length > 0) runs.push({ group, pieces: run, rank: Math.min(...ranks) });
      run = [];
    };
    for (const piece of sorted) {
      if (run.length > 0 && piece.index !== run[run.length - 1]!.index + 1) close();
      run.push(piece);
    }
    close();
  }
  runs.sort((left, right) => left.rank - right.rank);

  const citations: ChatCitationDto[] = [];
  const passages: string[] = [];
  for (const run of runs) {
    const best = [...run.group.hits.values()].find((entry) => entry.rank === run.rank)!.hit;
    passages.push(
      `[${String(citations.length + 1)}] ${sourceLabel(best.path, best.page, best.section)}\n${joinChunks(run.pieces)}`,
    );
    citations.push(citation(run.group.storeId, best, null));
  }

  const relatedPassages: string[] = [];
  const present = new Set(order.map((group) => `${group.storeId}\u0000${group.documentId}`));
  for (const { storeId, hit, relation } of related) {
    const text = hitText(hit);
    const key = `${storeId}\u0000${hit.documentId}`;
    if (!text || present.has(key)) continue;
    const passage = `[${String(citations.length + 1)}] ${sourceLabel(hit.path, hit.page, hit.section)} — связь «${relation}»\n${text}`;
    const cost =
      estimateTokens(`${passage}\n\n`) +
      (relatedPassages.length ? 0 : estimateTokens(`${RELATED_HEADING}\n`)) +
      (passages.length || relatedPassages.length ? 0 : blockCost);
    if (used + cost > budget) continue;
    used += cost;
    present.add(key);
    relatedPassages.push(passage);
    citations.push(citation(storeId, hit, relation));
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
  if (passages.length || relatedPassages.length) {
    const blocks = [
      ...(passages.length ? [passages.join("\n\n")] : []),
      ...(relatedPassages.length ? [`${RELATED_HEADING}\n${relatedPassages.join("\n\n")}`] : []),
    ];
    messages.push({ role: "user", content: `${SOURCES_HEADING}\n${blocks.join("\n\n")}` });
  }
  messages.push(current);
  return {
    messages,
    citations,
    tokensIn: used,
    trimmed: history.length - recent.length,
    neighbours: neighbourCount,
    related: relatedPassages.length,
  };
}

function citation(
  storeId: ChatCitationDto["storeId"],
  hit: VectorSearchHitDto,
  related: string | null,
): ChatCitationDto {
  return {
    storeId,
    documentId: hit.documentId,
    sourcePath: hit.path,
    chunkIndex: hit.chunkIndex,
    score: hit.score,
    section: hit.section,
    page: hit.page,
    related,
  };
}
