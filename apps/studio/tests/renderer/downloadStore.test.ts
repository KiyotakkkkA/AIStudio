import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  CatalogueItemDto,
  contract,
  DownloadDto,
  RunSummaryDto,
  type Contract,
  type DiskUsageDto,
  type RunCountsDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import DownloadStore from "../../src/renderer/features/downloads/DownloadStore.ts";
import {
  estimateEta,
  smoothRate,
  type RateState,
} from "../../src/renderer/features/progressRate.ts";
import {
  formatBytes,
  formatEta,
  formatRate,
  percentOf,
  shortChecksum,
} from "../../src/renderer/features/downloads/downloadPresentation.ts";

const DOWNLOAD = "0199cc11-1111-7111-8111-000000000001";
const RUN = "0199cc11-1111-7111-8111-000000000002";
const STREAM = "0199cc11-1111-7111-8111-000000000003";
const MIB = 1024 ** 2;
const START = 1_800_000_000_000;

const counts: RunCountsDto = {
  total: 1,
  byStatus: {
    queued: 0,
    running: 1,
    blocked: 0,
    succeeded: 0,
    failed: 0,
    cancelled: 0,
    interrupted: 0,
  },
  byKind: { chat: 0, scenario: 0, agentic: 0, job: 0, indexing: 0, download: 1, browser: 0 },
};

const disk: DiskUsageDto = {
  root: "D:\\zvs\\downloads",
  totalBytes: 1000 * 1024 ** 3,
  freeBytes: 588 * 1024 ** 3,
  usedBytes: 412 * 1024 ** 3,
  categories: [
    { category: "models", bytes: 306 * 1024 ** 3 },
    { category: "vectors", bytes: 72 * 1024 ** 3 },
    { category: "other", bytes: 34 * 1024 ** 3 },
  ],
  measuredAt: START,
};

function download(patch: Partial<DownloadDto> = {}): DownloadDto {
  return DownloadDto.parse({
    id: DOWNLOAD,
    itemKind: "model",
    itemRef: "curated:model:qwen2.5-vl-7b-instruct-q4_k_m",
    displayName: "Qwen2.5-VL 7B Instruct (OCR)",
    sizeBytes: 100 * MIB,
    bytesDone: 10 * MIB,
    status: "running",
    priority: 5,
    targetPath: "D:\\zvs\\downloads\\models\\qwen.gguf",
    url: "https://example.test/qwen.gguf",
    rateBytesPerSecond: 0,
    runId: RUN,
    createdAt: START,
    updatedAt: START,
    ...patch,
  });
}

function catalogueItem(patch: Partial<CatalogueItemDto> = {}): CatalogueItemDto {
  return CatalogueItemDto.parse({
    ref: "curated:embedding:bge-m3-f16",
    kind: "embedding",
    source: "curated",
    name: "bge-m3",
    displayName: "BGE-M3",
    description: "Многоязычные эмбеддинги.",
    sizeBytes: 1_157_671_200,
    url: "https://example.test/bge-m3.gguf",
    tags: ["BAAI"],
    state: "available",
    downloadable: true,
    ...patch,
  });
}

function setup(options: { rows?: DownloadDto[]; catalogue?: CatalogueItemDto[] } = {}) {
  let rows = options.rows ?? [download()];
  const bridge = createFakeBridge<Contract>()
    .handle("downloads.list", () => rows)
    .handle("downloads.catalogue", () => options.catalogue ?? [catalogueItem()])
    .handle("downloads.disk", () => disk)
    .handle("settings.get", ({ key }) => ({ key }))
    .handle("runs.list", () => ({
      items: [
        RunSummaryDto.parse({
          id: RUN,
          streamId: STREAM,
          kind: "download",
          subjectId: DOWNLOAD,
          title: "Загрузка",
          status: "running",
          progress: { done: 0, total: 1 },
          createdAt: START,
        }),
      ],
      counts,
    }));
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new DownloadStore(createIpcClient(contract, bridge), events);
  return {
    store,
    bridge,
    setRows(next: DownloadDto[]) {
      rows = next;
    },
    progress(seq: number, done: number, at: number) {
      events.dispatch({ streamId: STREAM, seq, ts: at, type: "progress", done, total: 100 * MIB });
    },
  };
}

const wait = (ms: number): Promise<void> => new Promise((resume) => setTimeout(resume, ms));

test("the smoothed rate is a pure function that settles on the real throughput", () => {
  const first = smoothRate(undefined, { value: 0, at: 0 });
  expect(first.perSecond).toBe(0);
  expect(first.sample).toEqual({ value: 0, at: 0 });

  let state: RateState = first;
  for (let tick = 1; tick <= 20; tick += 1)
    state = smoothRate(state, { value: tick * 10 * MIB, at: tick * 1000 });
  expect(state.perSecond).toBeGreaterThan(9.5 * MIB);
  expect(state.perSecond).toBeLessThanOrEqual(10 * MIB);

  const dipped = smoothRate(state, { value: 20 * 10 * MIB + MIB, at: 21_000 });
  expect(dipped.perSecond).toBeGreaterThan(5 * MIB);

  expect(smoothRate(state, { value: state.sample.value + 1, at: state.sample.at })).toBe(state);

  const resumed = smoothRate(state, { value: 0, at: 22_000 });
  expect(resumed.perSecond).toBe(0);
  expect(resumed.sample.value).toBe(0);
});

test("the estimate is only offered while something is actually moving", () => {
  expect(estimateEta(0, 100, 0)).toBeUndefined();
  expect(estimateEta(100, 100, 10)).toBeUndefined();
  expect(estimateEta(50, 100, 10)).toBe(5000);
});

test("byte, rate and percentage formatting reads the way the mockup draws it", () => {
  expect(formatBytes(0)).toBe("0 Б");
  expect(formatBytes(84 * 1024)).toBe("84 КБ");
  expect(formatBytes(1_157_671_200)).toBe("1.1 ГБ");
  expect(formatRate(0)).toBe("—");
  expect(formatRate(62 * MIB)).toBe("62 МБ/с");
  expect(formatEta(24 * 60 * 1000)).toBe("24 мин");
  expect(formatEta(65 * 60 * 1000)).toBe("1 ч 5 мин");
  expect(formatEta(undefined)).toBe("—");
  expect(percentOf(62, 100)).toBe(62);
  expect(percentOf(1, 0)).toBe(0);
  expect(shortChecksum("sha256", "4c1f0000000000000000000000009ab2")).toBe("sha256:4c1f…9ab2");
});

test("progress events move the row without a single extra list call", async () => {
  const { store, bridge, progress } = setup();
  await store.mount();
  const listed = bridge.calls.filter((call) => call.channel === "downloads.list").length;

  progress(1, 20 * MIB, START + 1000);
  progress(2, 30 * MIB, START + 2000);

  const row = store.downloads[0];
  expect(row?.bytesDone).toBe(30 * MIB);
  expect(store.rateOf(row!)).toBeGreaterThan(0);
  expect(store.etaOf(row!)).toBeGreaterThan(0);
  expect(bridge.calls.filter((call) => call.channel === "downloads.list").length).toBe(listed);

  store.dispose();
});

test("a gap in the stream is repaired by one reconciling read", async () => {
  const { store, bridge, setRows, progress } = setup();
  await store.mount();
  progress(1, 20 * MIB, START + 1000);
  const listed = bridge.calls.filter((call) => call.channel === "downloads.list").length;

  setRows([download({ bytesDone: 70 * MIB })]);
  progress(4, 60 * MIB, START + 4000);
  await wait(400);

  expect(bridge.calls.filter((call) => call.channel === "downloads.list").length).toBe(listed + 1);
  expect(store.downloads[0]?.bytesDone).toBe(70 * MIB);

  store.dispose();
});

test("an event on a stream the page has not mapped yet triggers one reconciling read", async () => {
  const { store, bridge } = setup();
  await store.mount();
  const listed = bridge.calls.filter((call) => call.channel === "downloads.list").length;

  store.receive({
    streamId: "0199cc11-1111-7111-8111-00000000000f" as never,
    seq: 1,
    ts: START,
    type: "progress",
    done: 1,
    total: 2,
  });
  await wait(400);

  expect(bridge.calls.filter((call) => call.channel === "downloads.list").length).toBe(listed + 1);
  store.dispose();
});

test("filters and counts are derived from the whole catalogue, not the filtered view", async () => {
  const { store } = setup({
    catalogue: [
      catalogueItem(),
      catalogueItem({
        ref: "curated:model:bge-reranker-v2-m3-f16",
        kind: "model",
        name: "bge-reranker-v2-m3",
        displayName: "BGE Reranker v2 M3",
        state: "installed",
        downloadable: false,
      }),
      catalogueItem({
        ref: "curated:model:qwen2.5-vl-7b-instruct-q4_k_m",
        kind: "model",
        name: "qwen2.5-vl:7b",
        displayName: "Qwen2.5-VL 7B Instruct (OCR)",
        state: "available",
        downloadable: false,
        blockedReason: "Недостаточно места на диске: нужно 5.7 ГБ, свободно 1.0 ГБ",
      }),
    ],
  });
  await store.mount();

  expect(store.kindCounts).toMatchObject({ model: 2, embedding: 1 });
  expect(store.stateCounts).toMatchObject({ available: 2, installed: 1 });

  store.setKind("model");
  expect(store.visible).toHaveLength(2);
  store.setState("installed");
  expect(store.visible.map((item) => item.name)).toEqual(["bge-reranker-v2-m3"]);
  store.setState("installed");
  expect(store.visible).toHaveLength(2);

  store.setKind("all");
  store.setQuery("ocr");
  expect(store.visible.map((item) => item.name)).toEqual(["qwen2.5-vl:7b"]);
  expect(store.visible[0]?.downloadable).toBe(false);

  store.dispose();
});

test("the detail rail reads one item however the row was reached", async () => {
  const { store } = setup({
    catalogue: [
      catalogueItem({
        ref: "curated:model:qwen2.5-vl-7b-instruct-q4_k_m",
        kind: "model",
        name: "qwen2.5-vl:7b",
        displayName: "Qwen2.5-VL 7B Instruct (OCR)",
        state: "downloading",
      }),
    ],
  });
  await store.mount();

  store.select("curated:model:qwen2.5-vl-7b-instruct-q4_k_m");
  expect(store.selectedItem?.name).toBe("qwen2.5-vl:7b");
  expect(store.selectedDownload?.id).toBe(DOWNLOAD);
  expect(store.optionsFor(store.selectedRef ?? "")).toEqual({ verifyChecksum: true });

  store.dispose();
});
