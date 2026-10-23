import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppError,
  AppErrorCode,
  ChatCitationDto,
  DocumentDto,
  ExplorerEntryDto,
  contract,
  type Contract,
  type DocumentSelectionDto,
  type Json,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createFakeClock } from "../../../../test/helpers/fakeClock.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ViewerStore, {
  VIEWER_DOCK_MAX_WIDTH,
  VIEWER_FIT_KEY,
  VIEWER_WIDTH_KEY,
  VIEWER_ZOOM_KEY,
} from "../../src/renderer/stores/ViewerStore.ts";
import ExplorerStore from "../../src/renderer/features/chat/explorer/ExplorerStore.ts";
import { openEntry, viewedPath } from "../../src/renderer/features/chat/explorer/openEntry.ts";
import ConversationVm from "../../src/renderer/features/chat/ConversationVm.ts";
import {
  MISSING_TITLE,
  SAVED,
  UNSUPPORTED_TITLE,
  viewerProblem,
} from "../../src/renderer/viewer/viewerCopy.ts";
import {
  PAGE_GAP,
  PAGE_PADDING,
  VIEWER_DOCK_MIN_WIDTH,
  clampPage,
  findMatches,
  fitWidthZoom,
  joinFolderPath,
  locatePassage,
  nextZoom,
  opensInViewer,
  pageAt,
  pageLayout,
  rangeAt,
  rangeOffsets,
  viewerCanShow,
  visiblePages,
} from "../../src/renderer/viewer/viewerModel.ts";

const DOCUMENT = "0199aa11-2222-7222-8222-222222222222";
const STORE = "0199aa11-1111-7111-8111-111111111111";

function documentDto(overrides: Record<string, unknown> = {}): DocumentDto {
  return DocumentDto.parse({
    id: DOCUMENT,
    contentHash: "hash",
    name: "Отчёт.pdf",
    sourcePath: "C:/docs/Отчёт.pdf",
    kind: "pdf",
    mimeType: "application/pdf",
    sizeBytes: 8,
    pageCount: 6,
    partCount: 1,
    open: true,
    openedAt: 1,
    ...overrides,
  });
}

function setup(seed: Record<string, Json> = {}) {
  const stored = new Map<string, Json>(Object.entries(seed));
  const clock = createFakeClock();
  const attached: DocumentSelectionDto[] = [];
  const revoked: string[] = [];
  const bridge = createFakeBridge<Contract>()
    .handle("settings.get", ({ key }) => ({ key, value: stored.get(key) }))
    .handle("settings.set", ({ key, value }) => {
      stored.set(key, value);
      return { key, value, updatedAt: clock.advance(1_000) };
    })
    .handle("documents.open", () => documentDto())
    .handle("documents.close", () => undefined)
    .handle("documents.page", ({ index }) => ({
      content: "bytes" as const,
      index,
      partCount: 1,
      offset: 0,
      totalBytes: 8,
      data: btoa("%PDF-1.7"),
    }));
  const viewer = new ViewerStore(
    createIpcClient(contract, bridge),
    { attachSelection: (selection) => attached.push(selection) },
    { create: () => "blob:image", revoke: (url) => revoked.push(url) },
  );
  return { viewer, bridge, stored, attached, revoked };
}

const calls = (bridge: ReturnType<typeof setup>["bridge"], channel: string) =>
  bridge.calls.filter((call) => call.channel === channel);

test("selection offsets map across text nodes and back to the same range", () => {
  const root = document.createElement("div");
  root.innerHTML = "<span>Выручка </span><b>11,12</b><span> млрд руб.</span>";
  const range = document.createRange();
  range.setStart(root.childNodes[0]!.firstChild!, 4);
  range.setEnd(root.childNodes[2]!.firstChild!, 5);
  const offsets = rangeOffsets(root, range);
  expect(offsets).toEqual({ start: 4, end: 18 });
  expect(rangeAt(root, offsets!.start, offsets!.end)?.toString()).toBe(range.toString());
  expect(range.toString()).toBe("чка 11,12 млрд");

  const elsewhere = document.createElement("p");
  elsewhere.textContent = "чужой текст";
  const outside = document.createRange();
  outside.selectNodeContents(elsewhere);
  expect(rangeOffsets(root, outside)).toBeNull();
  expect(rangeAt(root, 5, 5)).toBeNull();
});

test("a selection that starts before the page clamps to its first character", () => {
  const wrapper = document.createElement("div");
  wrapper.innerHTML = "<p>до</p><div><span>первая</span><span>вторая</span></div>";
  const page = wrapper.childNodes[1]!;
  const range = document.createRange();
  range.setStart(wrapper.firstChild!.firstChild!, 0);
  range.setEnd(page.childNodes[1]!.firstChild!, 3);
  expect(rangeOffsets(page, range)).toEqual({ start: 0, end: 9 });
});

test("a cited passage is found regardless of spacing, case and line breaks", () => {
  const page = "Глава 6\nВыручка  компании\nсоставила 3,36 млрд руб. в 2021 году.";
  const found = locatePassage(page, "выручка компании составила 3,36 МЛРД");
  expect(found).not.toBeNull();
  expect(page.slice(found!.start, found!.end)).toBe("Выручка  компании\nсоставила 3,36 млрд");
  expect(locatePassage(page, "нет такого текста")).toBeNull();
  expect(locatePassage(page, "   ")).toBeNull();
});

test("search finds every occurrence without overlapping", () => {
  expect(findMatches("Ааа ааа АААА", "аа")).toEqual([
    { start: 0, end: 2 },
    { start: 4, end: 6 },
    { start: 8, end: 10 },
    { start: 10, end: 12 },
  ]);
  expect(findMatches("текст", "  ")).toEqual([]);
});

test("virtualisation renders only the pages in view plus one either side", () => {
  const sizes = Array.from({ length: 200 }, () => ({ width: 600, height: 800 }));
  const layout = pageLayout(sizes, 1);
  expect(layout.slots).toHaveLength(200);
  expect(layout.slots[1]!.top).toBe(PAGE_PADDING + 800 + PAGE_GAP);
  expect(layout.height).toBe(PAGE_PADDING * 2 + 200 * 800 + 199 * PAGE_GAP);

  const top = layout.slots[99]!.top + 100;
  const visible = visiblePages(layout, top, 700);
  expect(visible).toEqual([99, 100, 101]);
  expect(visiblePages(layout, top + 600, 700)).toEqual([99, 100, 101, 102]);
  expect(visiblePages(layout, 0, 700)).toEqual([1, 2]);
  expect(visiblePages(pageLayout([], 1), 0, 700)).toEqual([]);
  expect(pageAt(layout, top, 700)).toBe(100);
});

test("zoom steps through presets and fit width leaves room for the page margin", () => {
  expect(nextZoom(1, 1)).toBe(1.25);
  expect(nextZoom(1, -1)).toBe(0.75);
  expect(nextZoom(3, 1)).toBe(4);
  expect(nextZoom(0.5, -1)).toBe(0.25);
  expect(fitWidthZoom(840, 600)).toBe(1);
  expect(fitWidthZoom(0, 600)).toBe(1);
  expect(clampPage(9, 6)).toBe(6);
  expect(clampPage(0, 6)).toBe(1);
});

test("width, zoom and fit persist per user and survive a restore", async () => {
  const { viewer, stored } = setup();
  viewer.setDockWidth(10);
  expect(viewer.dockWidth).toBe(VIEWER_DOCK_MIN_WIDTH);
  viewer.setDockWidth(640.4);
  viewer.commitDockWidth();
  viewer.setZoom(1.5);
  await Promise.resolve();
  await Promise.resolve();
  expect(stored.get(VIEWER_WIDTH_KEY)).toBe(640);
  expect(stored.get(VIEWER_ZOOM_KEY)).toBe(1.5);
  expect(stored.get(VIEWER_FIT_KEY)).toBe(false);

  const restored = setup(Object.fromEntries(stored)).viewer;
  await restored.restore();
  expect(restored.dockWidth).toBe(640);
  expect(restored.zoom).toBe(1.5);
  expect(restored.fitWidth).toBe(false);

  const broken = setup({ [VIEWER_WIDTH_KEY]: 99_999, [VIEWER_ZOOM_KEY]: "big" }).viewer;
  await broken.restore();
  expect(broken.dockWidth).toBe(VIEWER_DOCK_MAX_WIDTH);
  expect(broken.zoom).toBe(1);
  expect(broken.fitWidth).toBe(true);
});

test("a citation opens the document at the cited page with its passage", async () => {
  const { viewer, bridge } = setup();
  bridge
    .handle("documents.open", () => documentDto({ initialPage: 4 }))
    .handle("vectorStores.chunks", () => [
      {
        id: "chunk",
        payload: {},
        documentId: DOCUMENT,
        chunkIndex: 7,
        path: "C:/docs/Отчёт.pdf",
        text: "Выручка составила 11,12 млрд руб.",
        section: "",
        page: 4,
      },
    ]);
  const citation = ChatCitationDto.parse({
    storeId: STORE,
    documentId: DOCUMENT,
    sourcePath: "C:/docs/Отчёт.pdf",
    chunkIndex: 7,
    score: 0.9,
    page: 4,
  });
  await viewer.openCitation(citation, "База · стр. 4");

  expect(calls(bridge, "documents.open")[0]?.payload).toMatchObject({
    source: "citation",
    citation: { documentId: DOCUMENT, page: 4 },
  });
  expect(calls(bridge, "vectorStores.chunks")[0]?.payload).toEqual({
    storeId: STORE,
    documentId: DOCUMENT,
    indices: [7],
  });
  expect(viewer.status).toBe("ready");
  expect(viewer.page).toBe(4);
  expect(viewer.pageRequest?.page).toBe(4);
  expect(viewer.passage).toBe("Выручка составила 11,12 млрд руб.");
  expect(viewer.target?.location).toBe("База · стр. 4");
  expect(viewer.content).toMatchObject({ type: "pdf" });
  expect(viewer.content?.type === "pdf" && new TextDecoder().decode(viewer.content.bytes)).toBe(
    "%PDF-1.7",
  );
});

test("a citation without its chunk still opens at the page and highlights nothing", async () => {
  const { viewer, bridge } = setup();
  bridge
    .handle("documents.open", () => documentDto({ initialPage: 2 }))
    .fail("vectorStores.chunks", AppErrorCode.NOT_FOUND, "Хранилище не найдено");
  await viewer.openCitation(
    ChatCitationDto.parse({
      storeId: STORE,
      documentId: DOCUMENT,
      sourcePath: "C:/docs/Отчёт.pdf",
      chunkIndex: 0,
      score: 0.5,
      page: 2,
    }),
  );
  expect(viewer.status).toBe("ready");
  expect(viewer.page).toBe(2);
  expect(viewer.passage).toBeNull();
});

test("page navigation stays within the document", async () => {
  const { viewer } = setup();
  await viewer.openPath("C:/docs/Отчёт.pdf");
  viewer.setPageCount(3);
  viewer.nextPage();
  viewer.nextPage();
  viewer.nextPage();
  expect(viewer.page).toBe(3);
  viewer.requestPage(1);
  viewer.previousPage();
  expect(viewer.page).toBe(1);
  const nonce = viewer.pageRequest?.nonce;
  viewer.reportPage(2);
  expect(viewer.page).toBe(2);
  expect(viewer.pageRequest?.nonce).toBe(nonce);
});

test("text documents are assembled from every part", async () => {
  const { viewer, bridge } = setup();
  bridge
    .handle("documents.open", () =>
      documentDto({
        name: "notes.md",
        sourcePath: "C:/docs/notes.md",
        kind: "markdown",
        mimeType: "text/markdown",
        pageCount: undefined,
        partCount: 2,
      }),
    )
    .handle("documents.page", ({ index }) => ({
      content: "text" as const,
      index,
      partCount: 2,
      firstLine: index === 0 ? 1 : 3,
      encoding: "utf-8" as const,
      text: index === 0 ? "# План\n\n" : "Текст\n",
      untrusted: true as const,
    }));
  await viewer.openPath("C:/docs/notes.md");
  expect(viewer.content).toEqual({
    type: "text",
    text: "# План\n\nТекст\n",
    kind: "markdown",
    language: null,
  });
  expect(viewer.pageCount).toBeNull();
});

test("an office document loads its original bytes for in-app rendering", async () => {
  const { viewer, bridge } = setup();
  bridge.handle("documents.open", () =>
    documentDto({
      name: "Отчёт.xlsx",
      sourcePath: "C:/docs/Отчёт.xlsx",
      kind: "office",
      officeFormat: "xlsx",
      mimeType: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
      pageCount: undefined,
    }),
  );
  const opening = viewer.openPath("C:\\docs\\Отчёт.xlsx");
  expect(viewer.status).toBe("loading");
  expect(viewer.name).toBe("Отчёт.xlsx");
  await opening;
  expect(viewer.status).toBe("ready");
  expect(viewer.content).toMatchObject({ type: "xlsx" });
});

test("missing, unsupported and failed documents become distinct states", async () => {
  const { viewer, bridge } = setup();
  bridge.fail("documents.open", AppErrorCode.NOT_FOUND, "Файл не найден");
  await viewer.openPath("C:/docs/gone.pdf");
  expect(viewer.status).toBe("failed");
  expect(viewer.problem?.title).toBe(MISSING_TITLE);

  const unsupported = new AppError(AppErrorCode.UNSUPPORTED_FORMAT, "Формат не распознан");
  bridge.handle("documents.open", () => {
    throw unsupported;
  });
  await viewer.openPath("C:/docs/data.bin");
  expect(viewer.problem).toMatchObject({ kind: "unsupported", title: UNSUPPORTED_TITLE });

  bridge.fail("documents.open", AppErrorCode.LIMIT_EXCEEDED, "Файл слишком большой для просмотра");
  await viewer.openPath("C:/docs/huge.pdf");
  expect(viewer.problem).toMatchObject({
    kind: "tooLarge",
    hint: "Файл слишком большой для просмотра",
  });
  expect(viewerProblem(new Error("boom")).kind).toBe("failed");
});

test("a later open wins and the earlier document is released", async () => {
  const { viewer, bridge } = setup();
  let first!: (value: DocumentDto) => void;
  bridge.handle(
    "documents.open",
    () =>
      new Promise<DocumentDto>((resolve) => {
        first = resolve;
      }),
  );
  const slow = viewer.openPath("C:/docs/a.pdf");
  bridge.handle("documents.open", () =>
    documentDto({ id: "0199aa11-3333-7222-8222-222222222222", name: "b.pdf" }),
  );
  await viewer.openPath("C:/docs/b.pdf");
  first(documentDto({ name: "a.pdf" }));
  await slow;
  expect(viewer.document?.name).toBe("b.pdf");
  expect(calls(bridge, "documents.close").map((call) => call.payload)).toEqual([{ id: DOCUMENT }]);

  viewer.close();
  expect(viewer.isOpen).toBe(false);
  expect(calls(bridge, "documents.close").at(-1)?.payload).toEqual({
    id: "0199aa11-3333-7222-8222-222222222222",
  });
});

test("an image is shown through an object URL that is revoked on close", async () => {
  const { viewer, bridge, revoked } = setup();
  bridge.handle("documents.open", () =>
    documentDto({ name: "a.png", kind: "image", mimeType: "image/png", pageCount: undefined }),
  );
  await viewer.openPath("C:/docs/a.png");
  expect(viewer.content).toEqual({ type: "image", url: "blob:image" });
  viewer.close();
  expect(revoked).toEqual(["blob:image"]);
});

test("ask about this sends an untrusted selection DTO to the chat", async () => {
  const { viewer, attached } = setup();
  await viewer.openPath("C:/docs/Отчёт.pdf");
  expect(viewer.canAsk).toBe(false);
  expect(viewer.ask()).toBe(false);
  viewer.select({ page: 2, start: 10, end: 44, text: "  Выручка составила 11,12 млрд руб.\n" });
  expect(viewer.canAsk).toBe(true);
  expect(viewer.ask()).toBe(true);
  expect(attached).toEqual([
    {
      documentId: DOCUMENT,
      name: "Отчёт.pdf",
      page: 2,
      start: 10,
      end: 44,
      text: "Выручка составила 11,12 млрд руб.",
      untrusted: true,
    },
  ]);
  expect(viewer.selection).toBeNull();

  viewer.select({ page: null, start: 0, end: 4, text: "   " });
  expect(viewer.selection).toBeNull();
  viewer.select({ page: null, start: 0, end: 9000, text: "я".repeat(9000) });
  expect(viewer.selection?.text).toHaveLength(8000);
});

test("search counts wrap in both directions and reset with a new query", () => {
  const { viewer } = setup();
  viewer.openSearch();
  viewer.setQuery("выручка");
  viewer.setMatchCount(3);
  viewer.previousMatch();
  expect(viewer.matchIndex).toBe(2);
  viewer.nextMatch();
  expect(viewer.matchIndex).toBe(0);
  viewer.nextMatch();
  viewer.setQuery("доход");
  expect(viewer.matchIndex).toBe(0);
  viewer.closeSearch();
  expect(viewer.searchOpen).toBe(false);
  expect(viewer.query).toBe("");
});

test("download saves through the host and reports the outcome", async () => {
  const { viewer, bridge } = setup();
  bridge.handle("documents.save", () => ({ path: "C:/Users/a/Отчёт.pdf" }));
  await viewer.openPath("C:/docs/Отчёт.pdf");
  await viewer.save();
  expect(calls(bridge, "documents.save")[0]?.payload).toEqual({ id: DOCUMENT });
  expect(viewer.notice).toBe(SAVED);

  bridge.handle("documents.save", () => ({}));
  await viewer.save();
  expect(viewer.notice).toBeNull();
  bridge.fail("documents.save", AppErrorCode.NOT_FOUND, "Файл документа изменён или удалён");
  await viewer.save();
  expect(viewer.actionError).toBe("Файл документа изменён или удалён");
});

test("an artifact opens through its id, not its path", async () => {
  const { viewer, bridge } = setup();
  await viewer.openArtifact({
    id: "0199aa11-4444-7444-8444-444444444444/Отчёт.pdf",
    name: "Отчёт.pdf",
    format: "pdf",
    sizeBytes: 8,
  });
  expect(calls(bridge, "documents.open")[0]?.payload).toEqual({
    source: "artifact",
    artifactId: "0199aa11-4444-7444-8444-444444444444/Отчёт.pdf",
  });
});

test("the explorer sends rich formats to the viewer and text to its own view", async () => {
  const { viewer, bridge } = setup();
  bridge
    .handle("workspace.read", ({ path }) => ({
      path,
      size: 4,
      modifiedAt: null,
      text: "text",
      truncated: false,
    }))
    .handle("workspace.list", () => ({ path: "", entries: [], truncated: false }));
  const explorer = new ExplorerStore(
    createIpcClient(contract, bridge),
    createEventRouter({ logger: { log: () => {} } }),
  );
  const entry = (path: string) =>
    ExplorerEntryDto.parse({
      name: path.split("/").at(-1),
      path,
      kind: "file",
      size: 4,
      modifiedAt: null,
      ignored: false,
    });

  await openEntry(explorer, viewer, "C:\\work", entry("docs/Отчёт.pdf"));
  expect(calls(bridge, "documents.open").at(-1)?.payload).toEqual({
    source: "path",
    path: "C:\\work\\docs\\Отчёт.pdf",
  });
  expect(viewedPath(viewer, "C:\\work")).toBe("docs/Отчёт.pdf");
  expect(explorer.focused).toBe("docs/Отчёт.pdf");
  expect(explorer.openPath).toBeNull();

  await openEntry(explorer, viewer, "C:\\work", entry("notes.md"));
  expect(viewer.isOpen).toBe(false);
  expect(explorer.openPath).toBe("notes.md");
});

test("formats are routed by extension", () => {
  expect(opensInViewer("a/b/Отчёт.DOCX")).toBe(true);
  expect(opensInViewer("data.csv")).toBe(true);
  expect(opensInViewer("notes.md")).toBe(false);
  expect(opensInViewer("Makefile")).toBe(false);
  expect(viewerCanShow("md")).toBe(true);
  expect(viewerCanShow("zip")).toBe(false);
  expect(viewerCanShow("doc")).toBe(false);
  expect(joinFolderPath("C:\\work\\", "a/b.pdf")).toBe("C:\\work\\a\\b.pdf");
  expect(joinFolderPath("/home/u/work", "a/b.pdf")).toBe("/home/u/work/a/b.pdf");
});

test("the composer keeps up to eight distinct selections", () => {
  const composer = new ConversationVm();
  const selection = (start: number): DocumentSelectionDto => ({
    documentId: DOCUMENT as DocumentSelectionDto["documentId"],
    name: "Отчёт.pdf",
    page: 1,
    start,
    end: start + 5,
    text: "текст",
    untrusted: true,
  });
  expect(composer.addSelection(selection(0))).toBe(true);
  expect(composer.addSelection(selection(0))).toBe(true);
  expect(composer.selections).toHaveLength(1);
  for (let start = 1; start < 8; start += 1) composer.addSelection(selection(start * 10));
  expect(composer.selections).toHaveLength(8);
  expect(composer.addSelection(selection(500))).toBe(false);
  composer.removeSelection(0);
  expect(composer.selections[0]?.start).toBe(10);
  composer.clearSelections();
  expect(composer.selections).toEqual([]);
});
