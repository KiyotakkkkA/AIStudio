import { expect, test } from "vitest";
import { ChatCitationDto, ChatToolCallDto, DocumentSelectionDto, MessageId } from "@zvs/shared";
import {
  ALL_FRAGMENTS,
  answerCitations,
  citationSources,
  fragmentsLabel,
  questionSelections,
  selectionDetail,
  selectionSources,
  sourceDetail,
  sourceName,
} from "../../src/renderer/features/chat/citations.ts";
import { answerQuote } from "../../src/renderer/features/chat/answerQuoteModel.ts";
import { ANSWER_QUOTE_NAME } from "../../src/renderer/features/chat/selectionCopy.ts";

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

const DOC_A = "0199aa11-7777-7111-8111-00000000000a";
const DOC_B = "0199aa11-7777-7111-8111-00000000000b";

const picked = (documentId: string, page: number | null, start: number) =>
  DocumentSelectionDto.parse({
    documentId,
    name: documentId === DOC_A ? "Отчёт.pdf" : "Заметки.md",
    page,
    start,
    end: start + 5,
    text: "текст фрагмента",
    untrusted: true,
  });

test("selections group by document with their pages in order", () => {
  const sources = selectionSources([
    picked(DOC_A, 7, 0),
    picked(DOC_B, null, 0),
    picked(DOC_A, 2, 9),
  ]);
  expect(sources.map((source) => [source.name, source.pages, source.fragments])).toEqual([
    ["Отчёт.pdf", [2, 7], 2],
    ["Заметки.md", [], 1],
  ]);
  expect(sources[0]?.selection.page).toBe(2);
  expect(selectionDetail(sources[0]!)).toBe("выделено в документе · стр. 2, 7");
  expect(selectionDetail(sources[1]!)).toBe("выделено в документе");
});

test("an answer takes the fragments attached to the question before it", () => {
  const first = [picked(DOC_A, 1, 0)];
  const byAnswer = questionSelections([
    { id: "q1", role: "user", selections: first },
    { id: "a1", role: "assistant", selections: [] },
    { id: "q2", role: "user", selections: [] },
    { id: "a2", role: "assistant", selections: [] },
  ]);
  expect(byAnswer.get("a1")).toBe(first);
  expect(byAnswer.get("a2")).toEqual([]);
});

const ANSWER = MessageId.parse("0199aa11-3333-7333-8333-333333333333");

test("a selection inside an answer becomes a quote of that answer", () => {
  const root = document.createElement("div");
  root.innerHTML = "<p>Выручка <b>11,12</b> млрд руб.</p>";
  const range = document.createRange();
  range.setStart(root.firstChild!.firstChild!, 3);
  range.setEnd(root.firstChild!.lastChild!, 5);
  const quote = answerQuote(root, range, ANSWER);
  expect(quote).toEqual({
    documentId: null,
    messageId: ANSWER,
    name: ANSWER_QUOTE_NAME,
    page: null,
    start: 3,
    end: 18,
    text: "учка 11,12 млрд",
    untrusted: true,
  });
  expect(DocumentSelectionDto.safeParse(quote).success).toBe(true);

  const elsewhere = document.createElement("p");
  elsewhere.textContent = "чужой текст";
  const across = document.createRange();
  across.setStart(root.firstChild!.firstChild!, 0);
  across.setEnd(elsewhere.firstChild!, 3);
  expect(answerQuote(root, across, ANSWER)).toBeNull();
  range.collapse(true);
  expect(answerQuote(root, range, ANSWER)).toBeNull();
});

test("quotes of answers are not listed as document sources", () => {
  const root = document.createElement("div");
  root.textContent = "Вывод ответа";
  const range = document.createRange();
  range.selectNodeContents(root);
  const quote = answerQuote(root, range, ANSWER)!;
  const sources = selectionSources([quote, picked(DOC_A, 1, 0)]);
  expect(sources.map((source) => source.name)).toEqual(["Отчёт.pdf"]);
});
