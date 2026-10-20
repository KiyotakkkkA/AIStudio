import { expect, test } from "vitest";
import { ChatCitationDto, ChatToolCallDto } from "@zvs/shared";
import { answerCitations } from "../../src/renderer/features/chat/citations.ts";

const store = "0199aa11-2222-7111-8111-111111111111";

const cited = (documentId: string, chunkIndex: number) =>
  ChatCitationDto.parse({
    storeId: store,
    documentId,
    sourcePath: `docs/${documentId}.pdf`,
    chunkIndex,
    score: 0.8,
  });

const call = (status: string, meta: unknown) =>
  ChatToolCallDto.parse({
    id: "call_1",
    key: "vector.search",
    name: "vector__search",
    round: 0,
    arguments: { storeId: store, query: "x" },
    status,
    meta,
  });

test("search results a tool call reports for display join the answer's sources once", () => {
  const retrieved = [cited("a", 0)];
  const merged = answerCitations(retrieved, [
    call("succeeded", { citations: [cited("a", 0), cited("b", 2)] }),
    call("failed", { citations: [cited("c", 1)] }),
    call("succeeded", { citations: "broken" }),
    call("succeeded", null),
  ]);
  expect(merged.map((citation) => `${citation.documentId}:${citation.chunkIndex}`)).toEqual([
    "a:0",
    "b:2",
  ]);
  expect(answerCitations([], [])).toEqual([]);
});
