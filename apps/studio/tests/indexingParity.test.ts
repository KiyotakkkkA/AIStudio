import { existsSync, readFileSync } from "node:fs";
import { join, sep } from "node:path";
import { expect, test } from "vitest";
import {
  CreateVectorStoreInput,
  DocumentId,
  VectorStoreId,
  createBrandedId,
  type VectorEvalRunDto,
  type VectorIndexReportDto,
} from "@zvs/shared";
import { temporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { REPO_ROOT, STUDIO_ROOT } from "../../../test/helpers/paths.ts";
import { RustCore } from "../src/host/drivers/rust/RustCore.ts";
import { SidecarDriver } from "../src/host/drivers/sidecar/SidecarDriver.ts";
import type { EmbeddingDriver } from "../src/host/drivers/ai/ports.ts";
import {
  createImageExtractor,
  createPdfExtractor,
  IMAGE_EXTENSIONS,
  type OcrPort,
} from "../src/host/indexing/documents.ts";
import { IndexingService } from "../src/host/indexing/IndexingService.ts";
import { nativeTargetTriple } from "../src/host/platform/paths.ts";
import { VectorEvalService } from "../src/host/services/VectorEvalService.ts";
import { VectorStoreService } from "../src/host/services/VectorStoreService.ts";

interface Spec {
  corpus: string;
  dimension: number;
  chunkSize: number;
  chunkOverlap: number;
  k: number;
  ocr: { enabled: boolean; modelRef: string; language: "rus"; minCharsPerPage: number };
  ocrAnswers: Record<string, string>;
  cases: { question: string; expected: string[] }[];
  expected: unknown;
}

const spec = JSON.parse(
  readFileSync(join(REPO_ROOT, "test", "fixtures", "indexing", "parity.json"), "utf8"),
) as Spec;

const SIDECAR = join(
  REPO_ROOT,
  "target",
  "debug",
  process.platform === "win32" ? "zvs-jobd.exe" : "zvs-jobd",
);

const encoder = new TextEncoder();

function hashed(text: string, dimension: number): Float32Array {
  const counts = new Array<number>(dimension).fill(0);
  for (const token of text.toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? []) {
    let hash = 0x811c9dc5;
    for (const byte of encoder.encode(token)) {
      hash ^= byte;
      hash = Math.imul(hash, 16777619) >>> 0;
    }
    counts[hash % dimension]! += 1;
  }
  const norm = Math.sqrt(counts.reduce((sum, value) => sum + value * value, 0));
  if (norm === 0) {
    counts[0] = 1;
    return new Float32Array(counts);
  }
  return new Float32Array(counts.map((value) => value / norm));
}

function relative(path: string, corpus: string): string {
  return path.replaceAll(`${corpus}${sep}`, "").replaceAll("\\", "/");
}

function reportSummary(report: VectorIndexReportDto, corpus: string) {
  return {
    files: report.files,
    indexed: report.indexed,
    unchanged: report.unchanged,
    removed: report.removed,
    skipped: report.skipped,
    failed: report.failed,
    chunks: report.chunks,
    cancelled: report.cancelled,
    notes: report.notes.map((note) => relative(note, corpus)),
  };
}

function evalSummary(run: VectorEvalRunDto) {
  return Object.fromEntries(
    run.modes.map((mode) => [
      mode.mode,
      {
        hitAt1: mode.metrics.hitAt1,
        hitAt5: mode.metrics.hitAt5,
        recallAtK: mode.metrics.recallAtK,
        mrr: mode.metrics.mrr,
        ranks: mode.cases.map((entry) => entry.rank),
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

test.skipIf(!existsSync(SIDECAR))(
  "the mixed corpus indexes to what the Rust host indexed",
  async () => {
    const db = temporaryDatabase();
    const sidecar = new SidecarDriver({ binaryPath: SIDECAR, pingIntervalMs: 60_000 });
    try {
      const corpus = join(REPO_ROOT, ...spec.corpus.split("/"));
      const core = RustCore.fromPaths({
        nativeAddonPath: join(
          STUDIO_ROOT,
          "resources/native",
          nativeTargetTriple(),
          "zvs-core.node",
        ),
      });
      const provider = db.client.repositories.providers.create({
        name: "Embedding",
        kind: "ollama",
        baseUrl: "http://localhost:11434",
        capabilities: ["embedding"],
        createdAt: 1,
        updatedAt: 1,
      });
      const embedding: EmbeddingDriver = {
        dimensions: () => spec.dimension,
        capabilities: () => ({
          family: "openai-compatible",
          authModes: ["api"],
          streaming: false,
          liveModelList: false,
          embedding: true,
          image: false,
          honours: { temperature: false, topK: false, topP: false, maxOutputTokens: false },
          tools: "native",
        }),
        listModels: async () => [],
        embed: async (texts: readonly string[]) =>
          texts.map((text) => hashed(text, spec.dimension)),
      };
      const drivers = {
        ephemeralDriver: async () => ({ text: null, embedding, image: null }),
      };
      const stores = new VectorStoreService({
        data: db.client,
        core,
        drivers,
        directory: join(db.directory, "stores"),
        clock: () => 1000,
      });
      const indexing = new IndexingService({
        data: db.client,
        core,
        drivers,
        stores,
        clock: () => 1000,
      });
      const ocr: OcrPort = {
        readImage: async (_model, image) => spec.ocrAnswers[image.mediaType] ?? "",
      };
      const scratchDir = join(db.directory, "scratch");
      indexing.registry
        .register([".pdf"], createPdfExtractor({ jobs: sidecar, ocr, scratchDir }))
        .register(IMAGE_EXTENSIONS, createImageExtractor({ jobs: sidecar, ocr, scratchDir }));

      const store = await stores.create(
        CreateVectorStoreInput.parse({
          name: "Parity",
          embeddingProviderId: provider.id,
          embeddingModelId: "parity-hashing",
          dimension: spec.dimension,
          chunkSize: spec.chunkSize,
          chunkOverlap: spec.chunkOverlap,
          ocr: spec.ocr,
        }),
      );
      indexing.addSource({
        storeId: store.id,
        kind: "folder",
        path: corpus,
        include: [],
        exclude: [],
        recursive: true,
      });

      const report = await indexing.index({ storeId: store.id, full: false });

      const documents = indexing
        .listDocuments(store.id)
        .map((document) => ({
          path: relative(document.sourcePath, corpus),
          chunkCount: document.chunkCount,
          id: document.id,
        }))
        .sort((left, right) => (left.path < right.path ? -1 : left.path > right.path ? 1 : 0));
      const path = stores.storePath(store.id);
      const rows = [];
      for (const document of documents) {
        const indices = Array.from({ length: document.chunkCount }, (_, index) => index);
        for (const chunk of await core.vectorChunks(path, document.id, indices))
          rows.push({
            path: document.path,
            chunkIndex: chunk.chunkIndex,
            section: chunk.section,
            page: chunk.page,
            text: chunk.text,
          });
      }

      const evaluation = new VectorEvalService({ data: db.client, stores, clock: () => 5000 });
      const ids = new Map(documents.map((document) => [document.path, document.id]));
      for (const entry of spec.cases)
        evaluation.add({
          storeId: createBrandedId(VectorStoreId, store.id),
          question: entry.question,
          expectedDocumentIds: entry.expected.map((name) =>
            createBrandedId(DocumentId, ids.get(name)!),
          ),
        });
      const run = await evaluation.run({ storeId: store.id, k: spec.k });

      const actual = {
        report: reportSummary(report, corpus),
        documents: documents.map(({ path: documentPath, chunkCount }) => ({
          path: documentPath,
          chunkCount,
        })),
        rows,
        eval: evalSummary(run),
      };
      expect(
        close(actual, spec.expected),
        `the TypeScript host produced\n${JSON.stringify(actual, null, 2)}\nbut the spec pins\n${JSON.stringify(spec.expected, null, 2)}`,
      ).toBe(true);
    } finally {
      await sidecar.dispose();
      db.dispose();
    }
  },
  60_000,
);
