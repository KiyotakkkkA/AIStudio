import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  contract,
  VectorDocumentDto,
  VectorEvalRunDto,
  type Contract,
  type VectorEvalCaseDto,
  type VectorStoreId,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge";
import VectorEvalStore from "../../src/renderer/features/vector-stores/VectorEvalStore";
import { evalCaseRows, evalMetricRows } from "../../src/renderer/features/vector-stores/evalRows";
import { searchRows } from "../../src/renderer/features/vector-stores/searchRows";

const STORE = "0199bb11-1111-7111-8111-000000000099" as VectorStoreId;
const CATS = "0199bb11-1111-7111-8111-0000000000c1";
const DOGS = "0199bb11-1111-7111-8111-0000000000d1";
const GONE = "0199bb11-1111-7111-8111-0000000000e1";
const CASE = "0199bb11-1111-7111-8111-0000000000f1";

const documents = [CATS, DOGS].map((id, index) =>
  VectorDocumentDto.parse({
    id,
    storeId: STORE,
    sourcePath: index === 0 ? "C:/docs/cats.md" : "C:/docs/dogs.md",
    contentHash: id,
    chunkCount: 1,
    bytes: 1,
    indexedAt: 1,
  }),
);

const entry = {
  id: CASE,
  storeId: STORE,
  question: "Who purrs?",
  expectedDocumentIds: [CATS, GONE],
  createdAt: 1,
} as VectorEvalCaseDto;

const run = VectorEvalRunDto.parse({
  storeId: STORE,
  k: 10,
  ranAt: 2,
  durationMs: 1500,
  modes: [
    {
      mode: "vector",
      metrics: { cases: 1, hitAt1: 0, hitAt5: 1, recallAtK: 0.5, mrr: 0.25 },
      cases: [{ caseId: CASE, rank: 4, found: 1, expected: 2 }],
    },
    {
      mode: "rerank",
      metrics: { cases: 1, hitAt1: 1, hitAt5: 1, recallAtK: 0.5, mrr: 1 },
      cases: [{ caseId: CASE, rank: 1, found: 1, expected: 2 }],
      fallbacks: 0,
    },
  ],
});

function setup() {
  const bridge = createFakeBridge<Contract>();
  bridge.handle("vectorStores.eval.list", () => ({ cases: [entry], lastRun: run }));
  bridge.handle("vectorStores.eval.add", (input) => ({
    ...input,
    id: "0199bb11-1111-7111-8111-0000000000f2",
    createdAt: 3,
  }));
  bridge.handle("vectorStores.eval.remove", ({ id }) => ({ id, removed: true }));
  bridge.handle("vectorStores.eval.run", () => run);
  return { bridge, store: new VectorEvalStore(createIpcClient(contract, bridge)) };
}

test("metric rows line up the modes and sign the change", () => {
  const rows = evalMetricRows(run);
  expect(rows.map((row) => row.label)).toEqual(["hit@1", "hit@5", "recall@k", "MRR"]);
  expect(rows[3]).toMatchObject({ values: ["0.250", "1.000"], delta: "+0.750", improved: true });
  expect(rows[1]).toMatchObject({ delta: "+0.000", improved: null });
  expect(evalMetricRows({ ...run, modes: [run.modes[0]!] })[0]?.delta).toBeNull();
  expect(evalMetricRows(null)).toEqual([]);
});

test("case rows show where the expected document landed and name missing documents", () => {
  const [row] = evalCaseRows([entry], run, documents);
  expect(row?.expected).toEqual(["C:/docs/cats.md", `удалён · ${GONE.slice(0, 8)}`]);
  expect(row?.ranks.map((rank) => rank.label)).toEqual(["#4", "#1"]);
  const missed = VectorEvalRunDto.parse({
    ...run,
    modes: [
      {
        ...run.modes[0]!,
        cases: [{ caseId: CASE, rank: null, found: 0, expected: 2, error: "boom" }],
      },
    ],
  });
  expect(evalCaseRows([entry], missed, documents)[0]?.ranks[0]).toMatchObject({
    label: "ошибка",
    missed: true,
    error: "boom",
  });
  expect(evalCaseRows([entry], null, documents)[0]?.ranks).toEqual([]);
});

test("search rows carry both scores only when the hit was reranked", () => {
  const [plain, reranked] = searchRows(
    [
      { id: "a", score: 0.9, documentId: CATS, chunkIndex: 0, path: "a", payload: null },
      {
        id: "b",
        score: 0.7,
        vectorScore: 0.41,
        rerankScore: 0.7,
        documentId: DOGS,
        chunkIndex: 0,
        path: "b",
        payload: null,
      },
    ],
    0,
  );
  expect(plain).toMatchObject({ vectorScore: null, rerankScore: null });
  expect(reranked).toMatchObject({ score: "0.70", vectorScore: "0.41", rerankScore: "0.70" });
});

test("loading a store brings its cases and the kept result", async () => {
  const { store } = setup();
  await store.load(STORE);
  expect(store.cases).toEqual([entry]);
  expect(store.lastRun).toEqual(run);
  expect(store.canRun).toBe(true);
});

test("a case needs a question and at least one expected document", async () => {
  const { bridge, store } = setup();
  await store.load(STORE);
  store.set("question", "Who barks?");
  expect(store.canAdd).toBe(false);
  store.addExpected(DOGS);
  store.addExpected(DOGS);
  expect(store.expected).toEqual([DOGS]);
  expect(store.canAdd).toBe(true);
  expect(await store.add()).toBe(true);
  expect(store.cases.map((item) => item.question)).toEqual(["Who purrs?", "Who barks?"]);
  expect(store.question).toBe("");
  expect(store.expected).toEqual([]);
  expect(bridge.calls.at(-1)?.payload).toEqual({
    storeId: STORE,
    question: "Who barks?",
    expectedDocumentIds: [DOGS],
  });
  store.addExpected(CATS);
  store.removeExpected(CATS);
  expect(store.expected).toEqual([]);
});

test("removing and running go through the host and keep the latest result", async () => {
  const { bridge, store } = setup();
  await store.load(STORE);
  store.set("k", "5");
  expect(await store.run()).toBe(true);
  expect(bridge.calls.at(-1)).toEqual({
    channel: "vectorStores.eval.run",
    payload: { storeId: STORE, k: 5 },
  });
  expect(store.running).toBe(false);
  expect(await store.remove(CASE)).toBe(true);
  expect(store.cases).toEqual([]);
  expect(store.canRun).toBe(false);
});

test("a bad depth is refused locally and host failures land on the section", async () => {
  const { bridge, store } = setup();
  await store.load(STORE);
  store.set("k", "0");
  expect(await store.run()).toBe(false);
  expect(store.error).toContain("от 1 до 100");
  store.set("k", "10");
  bridge.fail("vectorStores.eval.run", AppErrorCode.CONFLICT, "Проверка уже идёт");
  expect(await store.run()).toBe(false);
  expect(store.error).toBe("Проверка уже идёт");
  store.reset();
  expect(store.cases).toEqual([]);
  expect(store.lastRun).toBeNull();
  expect(store.error).toBeNull();
});
