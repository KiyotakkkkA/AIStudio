import { expect, test } from "vitest";
import { ChatSourceDto, MessageDto, VectorStoreId } from "@zvs/shared";
import {
  sourcesOfKind,
  vectorCitations,
  webSourceList,
  withStoreCitations,
} from "../../src/renderer/features/chat/sources.ts";
import { linkCitations, linkWebSources } from "../../src/renderer/features/chat/citationMarkers.ts";

const first = VectorStoreId.parse("0199aa11-2222-7111-8111-111111111111");
const second = VectorStoreId.parse("0199aa11-2222-7111-8111-222222222222");

const vector = (storeId: string, documentId: string) =>
  ChatSourceDto.parse({
    kind: "vector",
    storeId,
    documentId,
    sourcePath: `docs/${documentId}.md`,
    chunkIndex: 0,
    score: 0.5,
  });

const web = (url: string) => ChatSourceDto.parse({ kind: "web", url, title: url });

test("each kind keeps its own order, so markers count within their kind", () => {
  const sources = [
    vector(first, "a"),
    web("https://one.example/"),
    vector(first, "b"),
    web("https://two.example/"),
  ];
  expect(vectorCitations(sources).map((citation) => citation.documentId)).toEqual(["a", "b"]);
  expect(webSourceList(sources).map((source) => source.url)).toEqual([
    "https://one.example/",
    "https://two.example/",
  ]);
  expect(vectorCitations(sources)[0]).not.toHaveProperty("kind");
  expect(sourcesOfKind(sources, "web")).toHaveLength(2);
  const text = linkCitations(linkWebSources("См. [2] и [[2]].", 2), 2);
  expect(text).toBe("См. [2](#cite-2) и [2](#web-2).");
});

test("a store's live hits replace only that store's citations", () => {
  const sources = [vector(first, "a"), vector(second, "b"), web("https://one.example/")];
  const updated = withStoreCitations(sources, first, vectorCitations([vector(first, "c")]));
  expect(updated.map((source) => (source.kind === "web" ? source.url : source.documentId))).toEqual(
    ["b", "https://one.example/", "c"],
  );
});

test("a message without sources parses to an empty list", () => {
  const message = MessageDto.parse({
    id: "0199aa11-3333-7111-8111-111111111111",
    conversationId: "0199aa11-2222-7111-8111-111111111111",
    role: "assistant",
    content: "",
    tokensIn: 0,
    tokensOut: 0,
    usageEstimated: true,
    durationMs: 0,
    runId: null,
    partial: false,
    createdAt: 1,
  });
  expect(message.sources).toEqual([]);
});
