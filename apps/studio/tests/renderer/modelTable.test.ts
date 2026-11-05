import { expect, test } from "vitest";
import { modelTableRows } from "../../src/renderer/features/providers/modelTableModel";
import { modelRows } from "../../src/renderer/features/providers/modelRows";
import { model, MODEL, OTHER_MODEL } from "./providerFixtures";

const rows = modelRows([
  model({ id: MODEL, externalId: "model-10", sizeBytes: 500, contextWindow: 32_000 }),
  model({
    id: OTHER_MODEL,
    externalId: "model-2",
    sizeBytes: 100,
    contextWindow: 128_000,
    available: false,
  }),
  model({ id: MODEL, externalId: "model-3", sizeBytes: null, contextWindow: null }),
]);

test("model names sort numerically and leave the source order unchanged", () => {
  expect(modelTableRows(rows, [], "all", "name", "ascending").map((row) => row.externalId)).toEqual(
    ["model-2", "model-3", "model-10"],
  );
  expect(
    modelTableRows(rows, [], "all", "name", "descending").map((row) => row.externalId),
  ).toEqual(["model-10", "model-3", "model-2"]);
  expect(rows.map((row) => row.externalId)).toEqual(["model-10", "model-2", "model-3"]);
});

test("numeric sorting compares values and keeps missing metadata last in both directions", () => {
  for (const direction of ["ascending", "descending"] as const) {
    const size = modelTableRows(rows, [], "all", "size", direction);
    const context = modelTableRows(rows, [], "all", "context", direction);
    expect(size.map((row) => row.sizeBytes)).toEqual(
      direction === "ascending" ? [100, 500, null] : [500, 100, null],
    );
    expect(context.map((row) => row.contextWindow)).toEqual(
      direction === "ascending" ? [32_000, 128_000, null] : [128_000, 32_000, null],
    );
  }
});

test("selected and availability filters intersect the existing filtered rows without changing selection", () => {
  const selected = ["model-10", "model-2"];
  expect(
    modelTableRows(rows, selected, "selected", "name", "ascending").map((row) => row.externalId),
  ).toEqual(["model-2", "model-10"]);
  expect(modelTableRows(rows.slice(0, 1), selected, "selected", "name", "ascending")).toEqual([
    rows[0],
  ]);
  expect(
    modelTableRows(rows, selected, "available", "name", "ascending").map((row) => row.externalId),
  ).toEqual(["model-3", "model-10"]);
  expect(modelTableRows(rows, selected, "unavailable", "name", "ascending")).toEqual([rows[1]]);
  expect(selected).toEqual(["model-10", "model-2"]);
  expect(modelTableRows([], selected, "selected", "name", "ascending")).toEqual([]);
});

test("availability sorting groups unavailable and available models with a stable name tie-break", () => {
  expect(
    modelTableRows(rows, [], "all", "availability", "ascending").map((row) => row.externalId),
  ).toEqual(["model-2", "model-3", "model-10"]);
  expect(
    modelTableRows(rows, [], "all", "availability", "descending").map((row) => row.externalId),
  ).toEqual(["model-3", "model-10", "model-2"]);
});
