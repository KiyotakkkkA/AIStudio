import { expect, test } from "vitest";
import { ChatCitationDto, ChatToolCallDto } from "@zvs/shared";
import {
  ALL_FRAGMENTS,
  answerCitations,
  citationSources,
  fragmentsLabel,
  sourceDetail,
  sourceName,
} from "../../src/renderer/features/chat/citations.ts";

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

test("sources group fragments by document with their store and pages", () => {
  const other = "0199aa11-3333-7111-8111-111111111111";
  const plan = String.raw`C:\docs\План.docx`;
  const sources = citationSources(
    [
      ChatCitationDto.parse({ ...cited("plan", 0), sourcePath: plan, page: 5 }),
      ChatCitationDto.parse({ ...cited("plan", 3), sourcePath: plan, page: 2 }),
      ChatCitationDto.parse({ ...cited("plan", 4), sourcePath: plan, page: 5 }),
      ChatCitationDto.parse({ ...cited("notes", 1), storeId: other }),
    ],
    (storeId) => (storeId === store ? "Регламенты" : "Заметки"),
  );
  expect(sources.map((source) => [source.name, sourceDetail(source), source.fragments])).toEqual([
    ["План.docx", "Регламенты · стр. 2, 5", 3],
    ["notes.pdf", "Заметки", 1],
  ]);
  expect(sourceName("a/b/c.md")).toBe("c.md");
});

test("the fragments link counts what the list leaves out", () => {
  expect(fragmentsLabel(4, 0)).toBe(`${ALL_FRAGMENTS} (4)`);
  expect(fragmentsLabel(1200, 3)).toBe(
    `${ALL_FRAGMENTS} (${(1200).toLocaleString("ru-RU")}) · ещё 3 источника`,
  );
  expect(fragmentsLabel(9, 5)).toBe(`${ALL_FRAGMENTS} (9) · ещё 5 источников`);
});
