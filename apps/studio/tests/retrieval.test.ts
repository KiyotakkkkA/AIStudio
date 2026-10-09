import { expect, test } from "vitest";
import { VectorStoreId, type VectorSearchHitDto } from "@zvs/shared";
import { estimateTokens } from "../src/host/services/tokenEstimate.ts";
import { mergeRetrieval } from "../src/host/services/retrievalMerge.ts";
import { isFollowUp, retrievalQuery } from "../src/host/services/retrievalQuery.ts";

const SAMPLES = [
  {
    name: "russian prose",
    tokens: 123,
    text: "Верховный суд Российской Федерации рассмотрел кассационную жалобу общества с ограниченной ответственностью на решение арбитражного суда и постановление апелляционного суда. Суд указал, что при разрешении спора нижестоящие инстанции неправильно применили нормы материального права, поскольку не учли условия договора поставки и фактические обстоятельства дела. Дело направлено на новое рассмотрение в суд первой инстанции.\n",
  },
  {
    name: "english prose",
    tokens: 80,
    text: "The Supreme Court reviewed the cassation appeal filed by the limited liability company against the decision of the arbitration court and the ruling of the appellate court. The court held that the lower courts had misapplied substantive law, because they did not take into account the terms of the supply agreement and the actual circumstances of the case. The case was remitted to the court of first instance for a new hearing.\n",
  },
  {
    name: "russian question",
    tokens: 42,
    text: "Спасибо! А что во втором пункте? Можешь объяснить подробнее, почему суд так решил и какие статьи Гражданского кодекса он применил?\n",
  },
  {
    name: "russian with numbers",
    tokens: 88,
    text: "Дело № А33-13638/2020: истец ООО «Ромашка» требовал взыскать 1 250 000 руб. по договору № 15/2019 от 01.03.2019 (см. ст. 309, 310 ГК РФ).\n",
  },
];

test.each(SAMPLES)(
  "token estimate is within 25% of the Qwen2.5 tokenizer on $name",
  ({ text, tokens }) => {
    const estimate = estimateTokens(text);
    expect(Math.abs(estimate - tokens) / tokens).toBeLessThanOrEqual(0.25);
  },
);

test("token estimate is script-aware and never cheaper than a CJK character each", () => {
  expect(estimateTokens("")).toBe(0);
  expect(estimateTokens("абв")).toBe(1);
  expect(estimateTokens("abcd")).toBe(1);
  expect(estimateTokens("最高法院")).toBe(4);
  expect(estimateTokens("привет мир")).toBeLessThan(Buffer.byteLength("привет мир", "utf8") / 4);
});

const STORE_A = VectorStoreId.parse("0199bb11-1111-7111-8111-00000000000a");
const STORE_B = VectorStoreId.parse("0199bb11-1111-7111-8111-00000000000b");

function hit(
  documentId: string,
  chunkIndex: number,
  score: number,
  extra: Partial<VectorSearchHitDto> = {},
): VectorSearchHitDto {
  return {
    id: `${documentId}:${String(chunkIndex)}`,
    documentId,
    chunkIndex,
    score,
    path: `docs/${documentId}.md`,
    text: `${documentId} ${String(chunkIndex)}`,
    section: "",
    page: null,
    payload: {},
    ...extra,
  };
}

const ids = (hits: readonly { hit: VectorSearchHitDto }[]) => hits.map(({ hit }) => hit.id);

test("merge orders across stores by score, so a stronger second store comes first", () => {
  const { hits } = mergeRetrieval([
    { storeId: STORE_A, hits: [hit("a", 0, 0.5), hit("b", 0, 0.4)], floor: 0.3 },
    { storeId: STORE_B, hits: [hit("c", 0, 0.9), hit("d", 0, 0.45)], floor: 0.3 },
  ]);
  expect(ids(hits)).toEqual(["c:0", "a:0", "d:0", "b:0"]);
  expect(hits[0]!.storeId).toBe(STORE_B);
});

test("merge drops hits below each store's own floor, except keyword-only hits", () => {
  const { hits, stats } = mergeRetrieval([
    {
      storeId: STORE_A,
      hits: [hit("a", 0, 0.5), hit("b", 0, 0.1), hit("k", 0, 0.05, { provenance: "keyword" })],
      floor: 0.3,
    },
    { storeId: STORE_B, hits: [hit("c", 0, 0.5)], floor: 0.6 },
  ]);
  expect(ids(hits)).toEqual(["a:0", "k:0"]);
  expect(stats).toMatchObject({ fetched: 4, belowFloor: 2 });
});

test("an off-topic turn merges to nothing", () => {
  const { hits, stats } = mergeRetrieval([
    { storeId: STORE_A, hits: [hit("a", 0, 0.12), hit("b", 0, 0.08)], floor: 0.3 },
  ]);
  expect(hits).toEqual([]);
  expect(stats).toEqual({ fetched: 2, belowFloor: 2, duplicates: 0, overCap: 0, overLimit: 0 });
});

test("merge keeps the store's own order, lifting fused hits to the score below them", () => {
  const { hits } = mergeRetrieval([
    {
      storeId: STORE_A,
      hits: [hit("code", 0, 0.2, { provenance: "keyword" }), hit("a", 0, 0.7)],
      floor: 0.3,
    },
    { storeId: STORE_B, hits: [hit("b", 0, 0.6)], floor: 0.3 },
  ]);
  expect(ids(hits)).toEqual(["code:0", "a:0", "b:0"]);
});

test("merge collapses adjacent chunks of one document and caps chunks per document", () => {
  const { hits, stats } = mergeRetrieval(
    [
      {
        storeId: STORE_A,
        hits: [
          hit("a", 4, 0.9),
          hit("a", 5, 0.85),
          hit("a", 3, 0.8),
          hit("a", 8, 0.75),
          hit("a", 12, 0.7),
          hit("a", 20, 0.65),
          hit("b", 4, 0.6),
        ],
        floor: 0.3,
      },
      { storeId: STORE_B, hits: [hit("a", 5, 0.88)], floor: 0.3 },
    ],
    { perDocument: 3 },
  );
  expect(hits.map(({ storeId, hit }) => [storeId, hit.id])).toEqual([
    [STORE_A, "a:4"],
    [STORE_B, "a:5"],
    [STORE_A, "a:8"],
    [STORE_A, "a:12"],
    [STORE_A, "b:4"],
  ]);
  expect(stats).toMatchObject({ duplicates: 2, overCap: 1 });
});

test("merge keeps the top N", () => {
  const { hits, stats } = mergeRetrieval(
    [
      {
        storeId: STORE_A,
        hits: Array.from({ length: 10 }, (_, index) =>
          hit(`d${String(index)}`, 0, 0.9 - index / 100),
        ),
        floor: 0.3,
      },
    ],
    { limit: 6 },
  );
  expect(hits).toHaveLength(6);
  expect(stats.overLimit).toBe(4);
});

test("short or pronoun-led turns are searched together with the previous question", () => {
  expect(isFollowUp("А что во втором пункте?")).toBe(true);
  expect(
    isFollowUp("Это касается и договора подряда, заключённого между сторонами в 2020 году?"),
  ).toBe(true);
  expect(
    isFollowUp("And what does it say about the termination of the agreement by either party?"),
  ).toBe(true);
  expect(
    isFollowUp("Какие сроки исковой давности установлены для споров по договору поставки товаров?"),
  ).toBe(false);
  expect(isFollowUp("Спасибо!")).toBe(false);
  expect(isFollowUp("ok, thanks")).toBe(false);
  expect(retrievalQuery("А что во втором пункте?", "Какие условия договора поставки?")).toBe(
    "Какие условия договора поставки?\nА что во втором пункте?",
  );
  expect(retrievalQuery("А что во втором пункте?", undefined)).toBe("А что во втором пункте?");
  expect(retrievalQuery("спасибо", "Какие условия договора поставки?")).toBe("спасибо");
  expect(retrievalQuery("Почему?", "x".repeat(5000))).toBe(`${"x".repeat(2000)}\nПочему?`);
});
