import assert from "node:assert/strict";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  CatalogueItemDto,
  contract,
  DownloadDto,
  type Contract,
  type DiskUsageDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import StoreProvider from "../../src/renderer/app/StoreProvider.tsx";
import DownloadsPage from "../../src/renderer/pages/DownloadsPage.tsx";

const GIB = 1024 ** 3;
const MIB = 1024 ** 2;
const START = 1_800_000_000_000;
const RUNNING = "0199cc22-1111-7111-8111-000000000001";
const QUEUED = "0199cc22-1111-7111-8111-000000000002";
const DONE = "0199cc22-1111-7111-8111-000000000003";

const emptyCounts = {
  total: 0,
  byStatus: {
    queued: 0,
    running: 0,
    blocked: 0,
    succeeded: 0,
    failed: 0,
    cancelled: 0,
    interrupted: 0,
  },
  byKind: { chat: 0, scenario: 0, agentic: 0, job: 0, indexing: 0, download: 0, browser: 0 },
};

const disk: DiskUsageDto = {
  root: "D:\\zvs\\downloads",
  totalBytes: 1000 * GIB,
  freeBytes: 588 * GIB,
  usedBytes: 412 * GIB,
  categories: [
    { category: "models", bytes: 306 * GIB },
    { category: "vectors", bytes: 72 * GIB },
  ],
  measuredAt: START,
};

const running = DownloadDto.parse({
  id: RUNNING,
  itemKind: "model",
  itemRef: "curated:model:qwen2.5-vl-7b-instruct-q4_k_m",
  displayName: "Qwen2.5-VL 7B Instruct (OCR)",
  sizeBytes: 4_683_072_032,
  bytesDone: 2_341_536_016,
  status: "running",
  priority: 5,
  targetPath: "D:\\zvs\\downloads\\models\\qwen.gguf",
  url: "https://example.test/qwen.gguf",
  rateBytesPerSecond: 62 * MIB,
  createdAt: START,
  updatedAt: START,
});

const queued = DownloadDto.parse({
  id: QUEUED,
  itemKind: "model",
  itemRef: "curated:model:bge-reranker-v2-m3-f16",
  displayName: "BGE Reranker v2 M3",
  sizeBytes: 1_159_776_896,
  bytesDone: 0,
  status: "queued",
  priority: 5,
  targetPath: "D:\\zvs\\downloads\\models\\reranker.gguf",
  url: "https://example.test/reranker.gguf",
  createdAt: START,
  updatedAt: START,
});

const installed = DownloadDto.parse({
  id: DONE,
  itemKind: "embedding",
  itemRef: "curated:embedding:bge-m3-f16",
  displayName: "BGE-M3",
  sizeBytes: 1_157_671_200,
  bytesDone: 1_157_671_200,
  status: "succeeded",
  priority: 6,
  targetPath: "D:\\zvs\\downloads\\embeddings\\bge-m3.gguf",
  url: "https://example.test/bge-m3.gguf",
  createdAt: START,
  updatedAt: START,
});

const catalogue = [
  CatalogueItemDto.parse({
    ref: "curated:model:qwen2.5-vl-7b-instruct-q4_k_m",
    kind: "model",
    source: "curated",
    name: "qwen2.5-vl:7b",
    displayName: "Qwen2.5-VL 7B Instruct (OCR)",
    description: "Локальное распознавание текста.",
    sizeBytes: 4_683_072_032,
    url: "https://example.test/qwen.gguf",
    tags: ["OCR"],
    state: "downloading",
    downloadable: false,
    downloadId: RUNNING,
  }),
  CatalogueItemDto.parse({
    ref: "curated:embedding:bge-m3-f16",
    kind: "embedding",
    source: "curated",
    name: "bge-m3",
    displayName: "BGE-M3",
    description: "Многоязычные эмбеддинги.",
    sizeBytes: 1_157_671_200,
    dimension: 1024,
    url: "https://example.test/bge-m3.gguf",
    tags: ["BAAI"],
    state: "installed",
    downloadable: false,
  }),
  CatalogueItemDto.parse({
    ref: "curated:model:bge-reranker-v2-m3-f16",
    kind: "model",
    source: "curated",
    name: "bge-reranker-v2-m3",
    displayName: "BGE Reranker v2 M3",
    description: "Переранжирование результатов поиска.",
    sizeBytes: 1_159_776_896,
    url: "https://example.test/reranker.gguf",
    tags: ["BAAI"],
    state: "available",
    downloadable: false,
    blockedReason: "Недостаточно места на диске: нужно 2.1 ГБ, свободно 0.4 ГБ",
  }),
];

function renderPage() {
  const bridge = createFakeBridge<Contract>()
    .handle("settings.get", ({ key }) => ({ key }))
    .handle("settings.set", ({ key, value }) => ({ key, value, updatedAt: START }))
    .handle("downloads.list", () => [running, queued, installed])
    .handle("downloads.catalogue", () => catalogue)
    .handle("downloads.disk", () => disk)
    .handle("downloads.pause", () => running)
    .handle("downloads.prioritise", () => queued)
    .handle("downloads.remove", () => ({ id: DONE, removed: true as const }))
    .handle("runs.list", () => ({ items: [], counts: emptyCounts }));
  render(
    <MemoryRouter>
      <StoreProvider
        environment={{ ipc: createIpcClient(contract, bridge), events: createEventRouter() }}
      >
        <DownloadsPage />
      </StoreProvider>
    </MemoryRouter>,
  );
  return bridge;
}

test("the page draws the transfer in flight, the queue behind it and the catalogue states", async () => {
  renderPage();
  await screen.findAllByText("Qwen2.5-VL 7B Instruct (OCR)");

  assert.ok(screen.getByText("50%"));
  assert.ok(screen.getByText("1 из 2 слотов"));
  assert.ok(screen.getByText(/1.1 ГБ · ждёт свободного слота · приоритет 5/));
  assert.ok(screen.getByText("установлено"));
  assert.ok(screen.getByText("Недостаточно места на диске: нужно 2.1 ГБ, свободно 0.4 ГБ"));
  assert.equal(screen.getByRole("button", { name: "Скачать" }).hasAttribute("disabled"), true);
  assert.ok(screen.getByText("Модели 306 ГБ"));
});

test("pausing a transfer and reordering the queue go through the host", async () => {
  const bridge = renderPage();
  await screen.findAllByText("Qwen2.5-VL 7B Instruct (OCR)");

  fireEvent.click(
    screen.getByRole("button", { name: "Приостановить загрузку Qwen2.5-VL 7B Instruct (OCR)" }),
  );
  await waitFor(() => {
    assert.deepEqual(bridge.calls.find((call) => call.channel === "downloads.pause")?.payload, {
      id: RUNNING,
    });
  });

  fireEvent.click(screen.getByRole("button", { name: "Поднять BGE Reranker v2 M3 в очереди" }));
  await waitFor(() => {
    assert.deepEqual(
      bridge.calls.find((call) => call.channel === "downloads.prioritise")?.payload,
      { id: QUEUED, priority: 6 },
    );
  });
});

test("the detail rail shows the destination and the checksum", async () => {
  renderPage();
  await screen.findAllByText("Qwen2.5-VL 7B Instruct (OCR)");

  fireEvent.click(screen.getByText("BGE-M3"));
  await screen.findByText("Многоязычные эмбеддинги.");
  assert.ok(screen.getByText("D:\\zvs\\downloads\\embeddings\\bge-m3.gguf"));
  assert.ok(screen.getByText("контрольная сумма не опубликована"));

  assert.equal(screen.queryByRole("switch", { name: "Сделать моделью по умолчанию" }), null);
});
