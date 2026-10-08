import { expect, test } from "vitest";
import {
  rankedDocuments,
  retrievalMetrics,
  scoreCase,
} from "../src/host/services/retrievalMetrics.ts";

test("chunk hits collapse to documents in first-seen order", () => {
  expect(
    rankedDocuments([
      { documentId: "a" },
      { documentId: "b" },
      { documentId: "a" },
      { documentId: "c" },
      { documentId: "b" },
    ]),
  ).toEqual(["a", "b", "c"]);
});

test("a case is scored by the rank of its first expected document within k", () => {
  expect(scoreCase({ ranked: ["x", "y", "a", "b"], expected: ["b", "a"] }, 10)).toEqual({
    rank: 3,
    found: 2,
    expected: 2,
  });
  expect(scoreCase({ ranked: ["x", "y", "a", "b"], expected: ["b", "a"] }, 3)).toEqual({
    rank: 3,
    found: 1,
    expected: 2,
  });
  expect(scoreCase({ ranked: ["x", "y"], expected: ["a"] }, 10)).toEqual({
    rank: null,
    found: 0,
    expected: 1,
  });
});

test("duplicate expected documents count once", () => {
  expect(scoreCase({ ranked: ["a"], expected: ["a", "a"] }, 5)).toEqual({
    rank: 1,
    found: 1,
    expected: 1,
  });
});

test("aggregate metrics follow their textbook definitions", () => {
  const metrics = retrievalMetrics([
    { rank: 1, found: 1, expected: 1 },
    { rank: 2, found: 1, expected: 2 },
    { rank: 5, found: 2, expected: 2 },
    { rank: 6, found: 1, expected: 1 },
    { rank: null, found: 0, expected: 1 },
  ]);
  expect(metrics.cases).toBe(5);
  expect(metrics.hitAt1).toBeCloseTo(1 / 5);
  expect(metrics.hitAt5).toBeCloseTo(3 / 5);
  expect(metrics.recallAtK).toBeCloseTo((1 + 0.5 + 1 + 1 + 0) / 5);
  expect(metrics.mrr).toBeCloseTo((1 + 1 / 2 + 1 / 5 + 1 / 6 + 0) / 5);
});

test("an empty set scores zero rather than dividing by it", () => {
  expect(retrievalMetrics([])).toEqual({ cases: 0, hitAt1: 0, hitAt5: 0, recallAtK: 0, mrr: 0 });
});

test("a better ranking moves every metric the right way", () => {
  const before = retrievalMetrics([
    scoreCase({ ranked: ["x", "y", "z", "q", "w", "a"], expected: ["a"] }, 10),
    scoreCase({ ranked: ["x", "b"], expected: ["b"] }, 10),
  ]);
  const after = retrievalMetrics([
    scoreCase({ ranked: ["a", "x"], expected: ["a"] }, 10),
    scoreCase({ ranked: ["b", "x"], expected: ["b"] }, 10),
  ]);
  expect(after.mrr).toBeGreaterThan(before.mrr);
  expect(after.hitAt1).toBeGreaterThan(before.hitAt1);
  expect(after.hitAt5).toBeGreaterThan(before.hitAt5);
  expect(after.mrr).toBe(1);
});
