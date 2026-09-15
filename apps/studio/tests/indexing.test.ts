import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import {
  AppError,
  AppErrorCode,
  CreateVectorStoreInput,
  type Timestamp,
  type VectorStoreDto,
} from "@zvs/shared";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { temporaryDirectory, type TemporaryDirectory } from "../../../test/helpers/paths.ts";
import { FakeCore } from "../../../test/helpers/FakeCore.ts";
import { VectorStoreService } from "../src/host/services/VectorStoreService.ts";
import { IndexingService } from "../src/host/indexing/IndexingService.ts";
import { createExtractorRegistry } from "../src/host/indexing/extraction.ts";
import { accepts } from "../src/host/indexing/patterns.ts";
import { createVectorStoreHandlers } from "../src/host/ipc/vectorStores.ts";
import { RunService } from "../src/host/services/RunService.ts";
import { NodeRegistry } from "../src/host/kernel/NodeRegistry.ts";
import { registerCoreNodes } from "../src/host/kernel/coreNodes.ts";
import { createEventBus } from "../src/host/platform/events.ts";
import type { AiDriver, EmbeddingDriver } from "../src/host/drivers/ai/ports.ts";

let db: TemporaryDatabase;
let workspace: TemporaryDirectory;
let core: FakeCore;
let stores: VectorStoreService;
let indexing: IndexingService;
let store: VectorStoreDto;
let corpus: string;
let rateLimits = 0;
const sleep = vi.fn(async (ms: number): Promise<void> => {
  void ms;
});

async function defaultEmbed(texts: readonly string[]): Promise<Float32Array[]> {
  if (rateLimits > 0) {
    rateLimits -= 1;
    throw new AppError(AppErrorCode.RATE_LIMITED, "slow down");
  }
  return texts.map((text) => new Float32Array([text.length, 1, 0]));
}

const embed = vi.fn(defaultEmbed);

const embedding: EmbeddingDriver = {
  embed,
  dimensions: () => 3,
  listModels: async () => [],
  capabilities: () => ({
    family: "openai-compatible",
    authModes: ["api"],
    streaming: false,
    liveModelList: false,
    embedding: true,
    image: false,
    honours: { temperature: false, topK: false, topP: false, maxOutputTokens: false },
  }),
};

const driver: AiDriver = { text: null, embedding, image: null };

function write(name: string, text: string): string {
  const path = join(corpus, name);
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, text, "utf8");
  return path;
}

function documents() {
  return db.client.repositories.vectorDocuments.listByStore(store.id);
}

function vectorRows() {
  return core.rows.get(stores.storePath(store.id)) ?? [];
}

beforeEach(async () => {
  db = temporaryDatabase();
  workspace = temporaryDirectory("studio-index-");
  corpus = join(workspace.path, "corpus");
  mkdirSync(corpus, { recursive: true });
  rateLimits = 0;
  core = new FakeCore();
  const provider = db.client.repositories.providers.create({
    name: "Embedding",
    kind: "ollama",
    baseUrl: "http://localhost:11434",
    capabilities: ["embedding"],
    createdAt: 1,
    updatedAt: 1,
  });
  stores = new VectorStoreService({
    data: db.client,
    core,
    drivers: { ephemeralDriver: async () => driver },
    directory: workspace.path,
    clock: () => 1000,
  });
  indexing = new IndexingService({
    data: db.client,
    core,
    drivers: { ephemeralDriver: async () => driver },
    stores,
    clock: () => 1000,
    batchSize: 2,
    concurrency: 1,
    backoffMs: 10,
    sleep,
  });
  store = await stores.create(
    CreateVectorStoreInput.parse({
      name: "Knowledge",
      embeddingProviderId: provider.id,
      embeddingModelId: "embed-model",
      dimension: 3,
      chunkSize: 4,
      chunkOverlap: 1,
    }),
  );
  vi.clearAllMocks();
  embed.mockReset();
  embed.mockImplementation(defaultEmbed);
  sleep.mockReset();
  sleep.mockResolvedValue(undefined);
});

afterEach(() => {
  db.dispose();
  workspace.dispose();
});

function addFolder(include: string[] = [], exclude: string[] = []) {
  return indexing.addSource({
    storeId: store.id,
    kind: "folder",
    path: corpus,
    include,
    exclude,
    recursive: true,
  });
}

test("a folder is walked, extracted, embedded and recorded end to end", async () => {
  write("guide.md", "alpha beta gamma delta epsilon zeta eta theta");
  write("nested/notes.txt", "one two three four five");
  write("table.csv", "name,role\nana,support\nbo,design");
  addFolder();
  const report = await indexing.index({ storeId: store.id, full: false });
  expect(report.indexed).toBe(3);
  expect(report.unchanged).toBe(0);
  expect(report.chunks).toBeGreaterThan(0);
  expect(
    documents()
      .map((row) => row.sourcePath)
      .sort(),
  ).toEqual(
    [
      join(corpus, "guide.md"),
      join(corpus, "nested", "notes.txt"),
      join(corpus, "table.csv"),
    ].sort(),
  );
  expect(vectorRows().length).toBe(report.chunks);
  expect(db.client.repositories.vectorStores.findById(store.id)?.lastIndexedAt).toBe(1000);
});

test("unchanged files are skipped on a re-index and re-embedded when forced", async () => {
  write("guide.md", "alpha beta gamma delta epsilon");
  addFolder();
  await indexing.index({ storeId: store.id, full: false });
  const first = embed.mock.calls.length;
  expect(first).toBeGreaterThan(0);
  embed.mockClear();

  const again = await indexing.index({ storeId: store.id, full: false });
  expect(again.unchanged).toBe(1);
  expect(again.indexed).toBe(0);
  expect(embed).not.toHaveBeenCalled();

  const forced = await indexing.index({ storeId: store.id, full: true });
  expect(forced.indexed).toBe(1);
  expect(embed).toHaveBeenCalled();
});

test("a changed file replaces exactly its own vectors", async () => {
  write("a.md", "alpha beta gamma delta epsilon zeta eta theta iota kappa");
  write("b.md", "stable text that never changes here");
  addFolder();
  await indexing.index({ storeId: store.id, full: false });
  const before = documents();
  const untouched = before.find((row) => row.sourcePath.endsWith("b.md"))!;
  const untouchedRows = core.rowsFor(stores.storePath(store.id), untouched.id);

  write("a.md", "short now");
  const report = await indexing.index({ storeId: store.id, full: false });
  expect(report.indexed).toBe(1);
  expect(report.unchanged).toBe(1);

  const after = documents();
  const changed = after.find((row) => row.sourcePath.endsWith("a.md"))!;
  expect(changed.id).toBe(before.find((row) => row.sourcePath.endsWith("a.md"))!.id);
  expect(changed.chunkCount).toBe(1);
  expect(core.rowsFor(stores.storePath(store.id), changed.id).length).toBe(1);
  expect(core.rowsFor(stores.storePath(store.id), untouched.id)).toEqual(untouchedRows);
});

test("a removed file loses its vectors and its metadata", async () => {
  write("keep.md", "one two three four");
  const doomed = write("gone.md", "five six seven eight");
  addFolder();
  await indexing.index({ storeId: store.id, full: false });
  const removedDocument = documents().find((row) => row.sourcePath === doomed)!;
  expect(core.rowsFor(stores.storePath(store.id), removedDocument.id).length).toBeGreaterThan(0);

  rmSync(doomed);
  const report = await indexing.index({ storeId: store.id, full: false });
  expect(report.removed).toBe(1);
  expect(documents().map((row) => row.sourcePath)).toEqual([join(corpus, "keep.md")]);
  expect(core.rowsFor(stores.storePath(store.id), removedDocument.id)).toEqual([]);
});

test("cancellation stops at a batch boundary and leaves the store consistent", async () => {
  for (let i = 0; i < 6; i += 1) write(`file-${String(i)}.md`, "alpha beta gamma delta epsilon");
  addFolder();
  const controller = new AbortController();
  let seen = 0;
  embed.mockImplementation(async (texts: readonly string[]) => {
    seen += 1;
    if (seen >= 2) controller.abort();
    return texts.map((text) => new Float32Array([text.length, 1, 0]));
  });
  const report = await indexing.index(
    { storeId: store.id, full: false },
    { signal: controller.signal },
  );
  expect(report.cancelled).toBe(true);
  expect(report.indexed).toBeLessThan(6);
  for (const document of documents()) {
    expect(core.rowsFor(stores.storePath(store.id), document.id).length).toBe(document.chunkCount);
  }
  const orphans = vectorRows().filter(
    (row) => !documents().some((document) => document.id === row.documentId),
  );
  expect(orphans).toEqual([]);
});

test("a rate limit is met with backoff rather than failure", async () => {
  write("guide.md", "alpha beta gamma delta epsilon zeta");
  addFolder();
  rateLimits = 2;
  const report = await indexing.index({ storeId: store.id, full: false });
  expect(report.failed).toBe(0);
  expect(report.indexed).toBe(1);
  expect(sleep).toHaveBeenCalledTimes(2);
  expect(sleep.mock.calls.map(([ms]) => ms)).toEqual([10, 20]);
});

test("a rate limit that outlives the attempt budget fails only that file", async () => {
  write("guide.md", "alpha beta gamma");
  write("other.md", "delta epsilon zeta");
  addFolder();
  const strict = new IndexingService({
    data: db.client,
    core,
    drivers: { ephemeralDriver: async () => driver },
    stores,
    clock: () => 1000,
    maxAttempts: 2,
    backoffMs: 1,
    sleep,
  });
  embed.mockImplementationOnce(async () => {
    throw new AppError(AppErrorCode.RATE_LIMITED, "slow down");
  });
  embed.mockImplementationOnce(async () => {
    throw new AppError(AppErrorCode.RATE_LIMITED, "slow down");
  });
  const report = await strict.index({ storeId: store.id, full: false });
  expect(report.failed).toBe(1);
  expect(report.indexed).toBe(1);
  expect(report.notes.some((note) => note.includes("Ошибка"))).toBe(true);
});

test("unsupported formats are skipped with a reason and never fail the run", async () => {
  write("manual.pdf", "%PDF-1.7");
  write("readme.md", "alpha beta gamma");
  addFolder();
  const report = await indexing.index({ storeId: store.id, full: false });
  expect(report.indexed).toBe(1);
  expect(report.skipped).toBe(1);
  expect(report.failed).toBe(0);
  expect(report.notes.join(" ")).toContain(".pdf");
});

test("include and exclude patterns choose what is walked", async () => {
  write("docs/guide.md", "alpha beta gamma");
  write("docs/skip.txt", "delta epsilon");
  write("node_modules/dep/index.md", "should not appear");
  addFolder(["**/*.md"], ["node_modules"]);
  const report = await indexing.index({ storeId: store.id, full: false });
  expect(report.indexed).toBe(1);
  expect(documents().map((row) => row.sourcePath)).toEqual([join(corpus, "docs", "guide.md")]);
});

test("progress counts embedded chunks against an estimate and ends on the real total", async () => {
  write("guide.md", "alpha beta gamma delta epsilon zeta eta theta");
  addFolder();
  const progress: { done: number; total: number }[] = [];
  const report = await indexing.index(
    { storeId: store.id, full: false },
    {
      onProgress: ({ done, total }) => {
        progress.push({ done, total });
      },
    },
  );
  expect(progress.length).toBeGreaterThan(1);
  expect(progress.at(-1)).toEqual({ done: report.chunks, total: report.chunks });
  expect(progress.every(({ done, total }) => done <= total)).toBe(true);
});

test("a document can be removed by hand and takes its vectors with it", async () => {
  write("guide.md", "alpha beta gamma delta");
  addFolder();
  await indexing.index({ storeId: store.id, full: false });
  const document = documents()[0]!;
  await indexing.removeDocument(store.id, document.id);
  expect(documents()).toEqual([]);
  expect(vectorRows()).toEqual([]);
  await expect(indexing.removeDocument(store.id, document.id)).rejects.toMatchObject({
    code: AppErrorCode.NOT_FOUND,
  });
});

test("sources are persisted, deduplicated by path and removable", async () => {
  const first = addFolder(["**/*.md"]);
  const again = addFolder(["**/*.txt"]);
  expect(again.id).toBe(first.id);
  expect(indexing.listSources(store.id)).toHaveLength(1);
  expect(indexing.listSources(store.id)[0]?.include).toEqual(["**/*.txt"]);
  indexing.removeSource(first.id);
  expect(indexing.listSources(store.id)).toEqual([]);
  await expect(indexing.index({ storeId: store.id, full: false })).rejects.toMatchObject({
    code: AppErrorCode.CONFLICT,
  });
});

test("two indexing runs of one store do not overlap", async () => {
  write("guide.md", "alpha beta gamma delta");
  addFolder();
  let release = (): void => undefined;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  embed.mockImplementationOnce(async (texts: readonly string[]) => {
    await held;
    return texts.map((text) => new Float32Array([text.length, 1, 0]));
  });
  const running = indexing.index({ storeId: store.id, full: false });
  await expect(indexing.index({ storeId: store.id, full: false })).rejects.toMatchObject({
    code: AppErrorCode.CONFLICT,
  });
  release();
  await running;
});

test("the extractor registry accepts new formats without editing the pipeline", async () => {
  const registry = createExtractorRegistry();
  expect(registry.supports("a.pdf")).toBe(false);
  expect(registry.reason("a.pdf")).toContain(".pdf");
  registry.register([".pdf"], async ({ bytes }) => `extracted ${String(bytes.byteLength)}`);
  expect(registry.supports("a.pdf")).toBe(true);
  await expect(registry.extract({ path: "a.pdf", bytes: new Uint8Array([1, 2, 3]) })).resolves.toBe(
    "extracted 3",
  );
});

test("csv rows are extracted as labelled lines", async () => {
  const registry = createExtractorRegistry();
  const text = await registry.extract({
    path: "people.csv",
    bytes: new TextEncoder().encode('name,role\n"ana, b",support\nbo,design'),
  });
  expect(text).toBe("name: ana, b; role: support\nname: bo; role: design");
});

test("patterns match by segment and treat a bare name as any depth", () => {
  expect(accepts("docs/guide.md", ["**/*.md"], [])).toBe(true);
  expect(accepts("docs/guide.txt", ["**/*.md"], [])).toBe(false);
  expect(accepts("guide.md", ["*.md"], [])).toBe(true);
  expect(accepts("node_modules/x/y.md", [], ["node_modules"])).toBe(false);
  expect(accepts("src/a.md", [], [])).toBe(true);
});

test("the vectorStores.index channel runs the job through the kernel and reports progress", async () => {
  write("guide.md", "alpha beta gamma delta epsilon zeta eta theta");
  addFolder();
  const events = createEventBus();
  const runs = new RunService({
    data: db.client,
    events,
    registry: registerCoreNodes(new NodeRegistry()),
    services: { indexing },
  });
  const handlers = createVectorStoreHandlers({ service: stores, indexing, runs });
  try {
    const handle = await handlers["vectorStores.index"]({ storeId: store.id, full: false });
    const seen: { done: number; total: number }[] = [];
    const stop = runs.subscribe(handle.id, (event) => {
      if (event.type === "progress") seen.push({ done: event.done, total: event.total });
    });
    await runs.wait(handle.id);
    stop();
    expect(runs.get(handle.id).status).toBe("succeeded");
    expect(runs.get(handle.id).kind).toBe("indexing");
    expect(seen.some(({ done }) => done > 0)).toBe(true);
    expect(documents()).toHaveLength(1);
    const step = runs.steps(handle.id)[0];
    expect(step?.output).toMatchObject({ indexed: 1, cancelled: false });
  } finally {
    await runs.dispose();
    events.dispose();
  }
});

test("the documents and sources channels answer the Documents tab", async () => {
  write("guide.md", "alpha beta gamma delta");
  const source = addFolder();
  const handlers = createVectorStoreHandlers({ service: stores, indexing });
  expect(await handlers["vectorStores.sources.list"]({ id: store.id })).toHaveLength(1);
  await indexing.index({ storeId: store.id, full: false });
  const listed = await handlers["vectorStores.documents.list"]({ id: store.id });
  expect(listed).toHaveLength(1);
  expect(listed[0]?.sourcePath).toBe(join(corpus, "guide.md"));
  await handlers["vectorStores.documents.remove"]({ storeId: store.id, id: listed[0]!.id });
  expect(await handlers["vectorStores.documents.list"]({ id: store.id })).toEqual([]);
  await handlers["vectorStores.sources.remove"]({ id: source.id });
  expect(await handlers["vectorStores.sources.list"]({ id: store.id })).toEqual([]);
  await expect(handlers["vectorStores.sources.pick"]({ kind: "folder" })).rejects.toMatchObject({
    code: AppErrorCode.CONFLICT,
  });
});

test("machine load is sampled while the pipeline runs and stops with it", async () => {
  write("guide.md", "alpha beta gamma delta");
  addFolder();
  let watching = 0;
  const sample = {
    at: 1000 as Timestamp,
    cpuPercent: 41,
    memoryUsedBytes: 8,
    memoryTotalBytes: 16,
    processMemoryBytes: 4,
    gpuName: "NVIDIA",
    gpuPercent: 77,
    vramUsedBytes: 2,
    vramTotalBytes: 8,
  };
  const watched = new IndexingService({
    data: db.client,
    core,
    drivers: { ephemeralDriver: async () => driver },
    stores,
    clock: () => 1000,
    sleep,
    resources: {
      watch(_intervalMs, onSample) {
        watching += 1;
        onSample(sample);
        return () => {
          watching -= 1;
        };
      },
    },
  });
  const samples: number[] = [];

  await watched.index(
    { storeId: store.id, full: false },
    { onSample: (taken) => samples.push(taken.gpuPercent ?? -1) },
  );

  expect(samples).toEqual([77]);
  expect(watching).toBe(0);
});

test("nothing is sampled when the caller does not ask for it", async () => {
  write("guide.md", "alpha beta");
  addFolder();
  let watching = 0;
  const watched = new IndexingService({
    data: db.client,
    core,
    drivers: { ephemeralDriver: async () => driver },
    stores,
    clock: () => 1000,
    sleep,
    resources: {
      watch() {
        watching += 1;
        return () => undefined;
      },
    },
  });

  await watched.index({ storeId: store.id, full: false });

  expect(watching).toBe(0);
});

test("each document is reported as its vectors land, not only when the run ends", async () => {
  write("first.md", "alpha beta gamma");
  write("second.md", "delta epsilon zeta");
  addFolder();
  const seen: { path: string; chunks: number }[] = [];

  const report = await indexing.index(
    { storeId: store.id, full: false },
    {
      onDocument: (document) => {
        seen.push({ path: document.sourcePath, chunks: document.chunkCount });
      },
    },
  );

  expect(report.indexed).toBe(2);
  expect(seen).toHaveLength(2);
  expect(seen.map((entry) => entry.path).sort()).toEqual(
    [join(corpus, "first.md"), join(corpus, "second.md")].sort(),
  );
  expect(seen.every((entry) => entry.chunks > 0)).toBe(true);
});

test("a file that is skipped or unchanged is never reported as a document", async () => {
  write("guide.md", "alpha beta gamma");
  write("photo.png", "not text");
  addFolder();
  await indexing.index({ storeId: store.id, full: false });

  const seen: string[] = [];
  const report = await indexing.index(
    { storeId: store.id, full: false },
    { onDocument: (document) => seen.push(document.sourcePath) },
  );

  expect(report.unchanged).toBe(1);
  expect(report.skipped).toBe(1);
  expect(seen).toEqual([]);
});

test("a reset empties the store but keeps it, its sources and its settings", async () => {
  write("guide.md", "alpha beta gamma");
  write("other.md", "delta epsilon zeta");
  const source = addFolder();
  const indexed = await indexing.index({ storeId: store.id, full: false });
  expect(indexed.indexed).toBe(2);
  expect(indexing.listDocuments(store.id)).toHaveLength(2);

  const cleared = await stores.clear(store.id);

  expect(cleared.id).toBe(store.id);
  expect(cleared.documents).toBe(0);
  expect(cleared.vectors).toBe(0);
  expect(cleared.lastIndexedAt).toBeNull();
  expect(indexing.listDocuments(store.id)).toEqual([]);
  // The configuration survives, so the next index refills from the same sources.
  expect(indexing.listSources(store.id).map((row) => row.id)).toEqual([source.id]);
  expect(cleared.chunkSize).toBe(4);
  expect(cleared.dimension).toBe(3);
  expect((await core.vectorStats(stores.storePath(store.id))).rowCount).toBe(0);
});

test("a store can be indexed again after a reset", async () => {
  write("guide.md", "alpha beta gamma");
  addFolder();
  await indexing.index({ storeId: store.id, full: false });
  await stores.clear(store.id);

  // Nothing is "unchanged" any more: the reset took the content hashes with the rows.
  const again = await indexing.index({ storeId: store.id, full: false });

  expect(again.indexed).toBe(1);
  expect(again.unchanged).toBe(0);
  expect(indexing.listDocuments(store.id)).toHaveLength(1);
});
