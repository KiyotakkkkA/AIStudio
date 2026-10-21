import { expect, test } from "vitest";
import { ChatToolCallDto, StepDto } from "@zvs/shared";
import { toolCallSummary } from "../../src/renderer/features/chat/toolCallSummary.ts";
import { citationNumber, linkCitations } from "../../src/renderer/features/chat/citationMarkers.ts";
import { retrievalHint } from "../../src/renderer/features/chat/retrievalModes.ts";
import { knowledgeSearchStep } from "../../src/renderer/features/tasks/knowledgeStep.ts";

const knowledgeCall = (key: string, args: unknown, meta: unknown = null) =>
  ChatToolCallDto.parse({
    id: "call_1",
    key,
    name: key.replace(".", "_"),
    round: 0,
    arguments: args,
    status: "succeeded",
    meta,
  });

test("citation markers become links only outside code and within the citation count", () => {
  const text =
    "Leave is 28 days [1], sick leave [2] and [9].\n```\nlist[1]\n```\nSee [x](https://a).";
  expect(linkCitations(text, 2)).toBe(
    "Leave is 28 days [1](#cite-1), sick leave [2](#cite-2) and [9].\n```\nlist[1]\n```\nSee [x](https://a).",
  );
  expect(linkCitations(text, 0)).toBe(text);
  expect(citationNumber("#cite-3")).toBe(3);
  expect(citationNumber("https://example.com")).toBeNull();
  expect(citationNumber(undefined)).toBeNull();
});

test("knowledge calls are summarised by their query and what was found", () => {
  expect(
    toolCallSummary(
      knowledgeCall(
        "knowledge.search",
        { query: "leave policy" },
        {
          citations: [
            {
              storeId: "0199aa11-2222-7111-8111-111111111111",
              documentId: "d",
              sourcePath: "a.md",
              chunkIndex: 0,
              score: 0.5,
            },
          ],
        },
      ),
    ),
  ).toBe("«leave policy» · найдено фрагментов: 1");
  expect(
    toolCallSummary(knowledgeCall("knowledge.read", { documentId: "d", section: "Отпуск" })),
  ).toBe("раздел «Отпуск»");
  expect(
    toolCallSummary(knowledgeCall("knowledge.read", { documentId: "d", fromChunk: 0, toChunk: 3 })),
  ).toBe("фрагменты 1–4");
  expect(toolCallSummary(knowledgeCall("demo.lookup", { q: "x" }))).toBe('{"q":"x"}');
});

test("a tool-less model falls back to searching before the answer", () => {
  expect(retrievalHint("agentic", "none")).toMatch(/перед ответом/);
  expect(retrievalHint("agentic", "native")).toMatch(/сама решает/);
  expect(retrievalHint("auto", "none")).toMatch(/Перед каждым ответом/);
});

test("a model-issued search step exposes its query and passages", () => {
  const step = StepDto.parse({
    id: "0199aa11-3333-7111-8111-111111111111",
    runId: "0199aa11-4444-7111-8111-111111111111",
    nodeId: "generate.call1",
    type: "knowledge.search",
    status: "succeeded",
    attempt: 1,
    startedAt: 1,
    finishedAt: 2,
    input: { query: "leave", scope: [] },
    output: {
      query: "leave",
      passages: [{ path: "a.md", section: "Leave", score: 0.7, text: "28 days" }],
    },
  });
  expect(knowledgeSearchStep(step)).toEqual({
    query: "leave",
    passages: [{ path: "a.md", section: "Leave", score: 0.7, text: "28 days" }],
  });
  expect(knowledgeSearchStep({ ...step, type: "vector.search" })).toBeNull();
});
