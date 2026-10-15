import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "vitest";
import {
  CreateVectorStoreInput,
  DocumentId,
  VectorStoreId,
  createBrandedId,
  type VectorEvalRunDto,
} from "@zvs/shared";
import { temporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { REPO_ROOT, STUDIO_ROOT } from "../../../test/helpers/paths.ts";
import { createFakeDriver } from "../../../test/helpers/FakeDriver.ts";
import { RustCore } from "../src/host/drivers/rust/RustCore.ts";
import { DEFAULT_ANN_THRESHOLD } from "../src/host/indexing/IndexingService.ts";
import { nativeTargetTriple } from "../src/host/platform/paths.ts";
import { VectorEvalService } from "../src/host/services/VectorEvalService.ts";
import { VectorStoreService } from "../src/host/services/VectorStoreService.ts";

interface Corpus {
  dimension: number;
  k: number;
  rerankModel: string;
  documents: { key: string; id: string; path: string; chunks: number }[];
  rows: {
    id: string;
    documentId: string;
    chunkIndex: number;
    path: string;
    text: string;
    vector: number[];
  }[];
  cases: { id: string; question: string; expected: string[]; vector: number[] }[];
  expected?: unknown;
}

const corpus = JSON.parse(
  readFileSync(join(REPO_ROOT, "test", "fixtures", "retrieval", "eval-corpus.json"), "utf8"),
) as Corpus;

function tokens(text: string): Set<string> {
  return new Set(text.toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? []);
}

function tokenOverlap(query: string, passage: string): number {
  const words = tokens(passage);
  return [...tokens(query)].filter((token) => words.has(token)).length;
}

function summary(run: VectorEvalRunDto) {
  return Object.fromEntries(
    run.modes.map((mode) => [
      mode.mode,
      {
        hitAt1: mode.metrics.hitAt1,
        hitAt5: mode.metrics.hitAt5,
        recallAtK: mode.metrics.recallAtK,
        mrr: mode.metrics.mrr,
        ranks: mode.cases.map((entry) => entry.rank),
        fallbacks: mode.fallbacks,
      },
    ]),
  );
}

function close(actual: unknown, expected: unknown): boolean {
  if (typeof actual === "number" && typeof expected === "number")
    return Math.abs(actual - expected) < 1e-9;
  if (Array.isArray(actual) && Array.isArray(expected))
    return (
      actual.length === expected.length &&
      actual.every((entry, index) => close(entry, expected[index]))
    );
  if (
    actual !== null &&
    expected !== null &&
    typeof actual === "object" &&
    typeof expected === "object"
  ) {
    const left = Object.entries(actual);
    return (
      left.length === Object.keys(expected).length &&
      left.every(([key, value]) => close(value, (expected as Record<string, unknown>)[key]))
    );
  }
  return actual === expected;
}

test("the fixture corpus scores what the Rust host scored", async () => {
  const db = temporaryDatabase();
  try {
    const core = RustCore.fromPaths({
      nativeAddonPath: join(STUDIO_ROOT, "resources/native", nativeTargetTriple(), "zvs-core.node"),
    });
    const provider = db.client.repositories.providers.create({
      name: "Embedding",
      kind: "ollama",
      baseUrl: "http://localhost:11434",
      capabilities: ["embedding"],
      createdAt: 1,
      updatedAt: 1,
    });
    const vectors = new Map(corpus.cases.map((entry) => [entry.question, entry.vector]));
    const fake = createFakeDriver();
    const stores = new VectorStoreService({
      data: db.client,
      core,
      directory: join(db.directory, "stores"),
      clock: () => 1000,
      drivers: {
        ephemeralDriver: async () => ({
          ...fake,
          embedding: {
            dimensions: () => corpus.dimension,
            capabilities: () => fake.text.capabilities(),
            listModels: async () => [],
            embed: async (texts: readonly string[]) =>
              texts.map((text) => new Float32Array(vectors.get(text)!)),
          },
        }),
      },
      rerankers: () => ({
        rerank: async (query, passages) => passages.map((passage) => tokenOverlap(query, passage)),
      }),
    });
    const store = await stores.create(
      CreateVectorStoreInput.parse({
        name: "Parity",
        embeddingProviderId: provider.id,
        embeddingModelId: "fixture-embedding",
        dimension: corpus.dimension,
        rerank: { enabled: true, modelRef: corpus.rerankModel, candidates: 50 },
      }),
    );
    for (const document of corpus.documents)
      db.client.repositories.vectorDocuments.recordIndexed({
        id: document.id,
        storeId: store.id,
        sourcePath: document.path,
        contentHash: document.key,
        chunkCount: document.chunks,
        bytes: 1,
        indexedAt: 1,
      });
    const path = stores.storePath(store.id);
    await core.upsertVectors(
      path,
      corpus.rows.map((row) => ({
        id: row.id,
        vector: row.vector,
        payload: { chunkCount: 2 },
        documentId: row.documentId,
        chunkIndex: row.chunkIndex,
        path: row.path,
        text: row.text,
        section: "",
        page: null,
      })),
    );
    await core.maintainVectorIndex(path, DEFAULT_ANN_THRESHOLD);
    await stores.reconcile(store.id);

    const evaluation = new VectorEvalService({ data: db.client, stores, clock: () => 5000 });
    for (const entry of corpus.cases)
      evaluation.add({
        storeId: createBrandedId(VectorStoreId, store.id),
        question: entry.question,
        expectedDocumentIds: entry.expected.map((id) => createBrandedId(DocumentId, id)),
      });
    const run = await evaluation.run({ storeId: store.id, k: corpus.k });
    const actual = summary(run);
    expect(
      close(actual, corpus.expected),
      `the TypeScript host scored\n${JSON.stringify(actual, null, 2)}\nbut the corpus pins\n${JSON.stringify(corpus.expected, null, 2)}`,
    ).toBe(true);
  } finally {
    db.dispose();
  }
});
