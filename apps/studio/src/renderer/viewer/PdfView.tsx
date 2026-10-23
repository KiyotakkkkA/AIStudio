import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import type ViewerStore from "../stores/ViewerStore";
import type { PdfDocument } from "./pdf";
import { RENDERING, RENDER_FAILED } from "./viewerCopy";
import {
  CURRENT_HIGHLIGHT,
  MATCH_HIGHLIGHT,
  PASSAGE_HIGHLIGHT,
  clearPaint,
  paint,
} from "./highlights";
import {
  PDF_TO_CSS,
  fitWidthZoom,
  findMatches,
  locatePassage,
  pageAt,
  pageLayout,
  rangeAt,
  visiblePages,
  type PageSize,
  type TextRange,
} from "./viewerModel";
import { PAGE_ATTRIBUTE, scrollRangeIntoView, useSelectionCapture } from "./useDocumentText";

interface PageMatch extends TextRange {
  readonly page: number;
}

const PdfPageView = memo(function PdfPageView({
  pdf,
  page,
  scale,
  width,
  height,
  onLayer,
}: {
  readonly pdf: PdfDocument;
  readonly page: number;
  readonly scale: number;
  readonly width: number;
  readonly height: number;
  readonly onLayer: (page: number, layer: HTMLElement | null) => void;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const layerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    let task: { cancel(): void } | null = null;
    let textLayer: { cancel(): void } | null = null;
    const canvas = canvasRef.current;
    const layer = layerRef.current;
    if (canvas === null || layer === null) return;
    void (async () => {
      const { TextLayer } = (await import("./pdf")).pdfLibrary();
      const proxy = await pdf.getPage(page);
      if (cancelled) return;
      const viewport = proxy.getViewport({ scale });
      const ratio = window.devicePixelRatio || 1;
      canvas.width = Math.floor(viewport.width * ratio);
      canvas.height = Math.floor(viewport.height * ratio);
      const context = canvas.getContext("2d");
      if (context === null) return;
      const render = proxy.render({
        canvas,
        canvasContext: context,
        viewport,
        transform: ratio === 1 ? undefined : [ratio, 0, 0, ratio, 0, 0],
      });
      task = render;
      const content = await proxy.getTextContent();
      if (cancelled) return;
      layer.replaceChildren();
      layer.style.setProperty("--total-scale-factor", String(scale));
      layer.style.setProperty("--scale-factor", String(scale));
      layer.style.setProperty("--scale-round-x", "1px");
      layer.style.setProperty("--scale-round-y", "1px");
      const text = new TextLayer({ textContentSource: content, container: layer, viewport });
      textLayer = text;
      await text.render();
      if (!cancelled) onLayer(page, layer);
      await render.promise.catch(() => undefined);
    })().catch(() => undefined);
    return () => {
      cancelled = true;
      task?.cancel();
      textLayer?.cancel();
      onLayer(page, null);
    };
  }, [pdf, page, scale, onLayer]);

  return (
    <div
      {...{ [PAGE_ATTRIBUTE]: page }}
      className="absolute left-1/2 -translate-x-1/2 overflow-hidden rounded-[3px] bg-white shadow-pop"
      style={{ width: `${String(width)}px`, height: `${String(height)}px` }}
    >
      <canvas
        ref={canvasRef}
        aria-hidden="true"
        className="block"
        style={{ width: `${String(width)}px`, height: `${String(height)}px` }}
      />
      <div ref={layerRef} className="textLayer" />
    </div>
  );
});

function PdfView({ viewer, bytes }: { readonly viewer: ViewerStore; readonly bytes: Uint8Array }) {
  const scrollerRef = useRef<HTMLDivElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const [pdf, setPdf] = useState<PdfDocument | null>(null);
  const [sizes, setSizes] = useState<PageSize[]>([]);
  const [texts, setTexts] = useState<string[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [frame, setFrame] = useState({ top: 0, width: 0, height: 0 });
  const [layers, setLayers] = useState<ReadonlyMap<number, HTMLElement>>(new Map());
  const passageShown = useRef<string | null>(null);
  const matchShown = useRef<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let destroy: (() => Promise<void>) | null = null;
    void (async () => {
      const { pdfLibrary } = await import("./pdf");
      const task = pdfLibrary().getDocument({ data: bytes.slice(), useSystemFonts: true });
      destroy = () => task.destroy();
      const document = await task.promise;
      if (cancelled) return;
      const measured: PageSize[] = [];
      for (let page = 1; page <= document.numPages; page += 1) {
        const viewport = (await document.getPage(page)).getViewport({ scale: 1 });
        measured.push({ width: viewport.width, height: viewport.height });
      }
      if (cancelled) return;
      setPdf(document);
      setSizes(measured);
      viewer.setPageCount(document.numPages);
    })().catch(() => {
      if (!cancelled) setFailed(true);
    });
    return () => {
      cancelled = true;
      void destroy?.();
      clearPaint();
    };
  }, [bytes, viewer]);

  const needsText = viewer.query.trim().length > 0 || viewer.passage !== null;
  useEffect(() => {
    if (pdf === null || texts !== null || !needsText) return;
    let cancelled = false;
    void (async () => {
      const collected: string[] = [];
      for (let page = 1; page <= pdf.numPages; page += 1) {
        const content = await (await pdf.getPage(page)).getTextContent();
        collected.push(content.items.map((item) => ("str" in item ? item.str : "")).join(""));
      }
      if (!cancelled) setTexts(collected);
    })().catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [pdf, texts, needsText]);

  useLayoutEffect(() => {
    const scroller = scrollerRef.current;
    if (scroller === null) return;
    const measure = () =>
      setFrame({
        top: scroller.scrollTop,
        width: scroller.clientWidth,
        height: scroller.clientHeight,
      });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(scroller);
    return () => observer.disconnect();
  }, [pdf]);

  const firstWidth = sizes[0]?.width ?? 0;
  useEffect(() => {
    if (viewer.fitWidth && frame.width > 0 && firstWidth > 0)
      viewer.applyFitZoom(fitWidthZoom(frame.width, firstWidth));
  }, [viewer, viewer.fitWidth, frame.width, firstWidth]);

  const scale = viewer.zoom * PDF_TO_CSS;
  const layout = useMemo(() => pageLayout(sizes, scale), [sizes, scale]);
  const visible = visiblePages(layout, frame.top, frame.height || 800);

  const request = viewer.pageRequest;
  useEffect(() => {
    const scroller = scrollerRef.current;
    const slot = request === null ? undefined : layout.slots[request.page - 1];
    if (scroller === null || slot === undefined) return;
    scroller.scrollTo({ top: slot.top - 8 });
  }, [request, layout]);

  const onLayer = useCallback((page: number, layer: HTMLElement | null) => {
    setLayers((current) => {
      const next = new Map(current);
      if (layer === null) next.delete(page);
      else next.set(page, layer);
      return next;
    });
  }, []);

  const matches = useMemo<PageMatch[]>(() => {
    if (texts === null) return [];
    return texts.flatMap((text, index) =>
      findMatches(text, viewer.query).map((match) => ({ ...match, page: index + 1 })),
    );
  }, [texts, viewer.query]);

  useEffect(() => {
    viewer.setMatchCount(matches.length);
  }, [viewer, matches.length]);

  const current = matches[viewer.matchIndex];
  useEffect(() => {
    if (current !== undefined) viewer.requestPage(current.page);
  }, [viewer, current]);

  const passage = useMemo(() => {
    if (texts === null || viewer.passage === null) return null;
    const order = [viewer.page - 1, ...texts.keys()].filter(
      (index, position, list) =>
        index >= 0 && index < texts.length && list.indexOf(index) === position,
    );
    for (const index of order) {
      const located = locatePassage(texts[index]!, viewer.passage);
      if (located !== null) return { ...located, page: index + 1 };
    }
    return null;
  }, [texts, viewer.passage, viewer.page]);

  useEffect(() => {
    if (passage === null || passageShown.current === viewer.passage) return;
    passageShown.current = viewer.passage;
    if (passage.page !== viewer.page) viewer.requestPage(passage.page);
  }, [viewer, passage]);

  useEffect(() => {
    const ranges = (list: readonly PageMatch[]) =>
      list.flatMap((match) => {
        const layer = layers.get(match.page);
        const range = layer === undefined ? null : rangeAt(layer, match.start, match.end);
        return range === null ? [] : [range];
      });
    paint(MATCH_HIGHLIGHT, ranges(matches));
    const focused = current === undefined ? [] : ranges([current]);
    paint(CURRENT_HIGHLIGHT, focused);
    const scroller = scrollerRef.current;
    const key =
      current === undefined
        ? null
        : `${viewer.query}:${String(current.page)}:${String(current.start)}`;
    if (focused[0] !== undefined && scroller !== null && matchShown.current !== key) {
      matchShown.current = key;
      scrollRangeIntoView(scroller, focused[0]);
    }
    paint(PASSAGE_HIGHLIGHT, passage === null ? [] : ranges([passage]));
  }, [viewer, layers, matches, current, passage]);

  useSelectionCapture(viewer, rootRef);

  if (failed)
    return <p className="m-auto px-6 text-center text-[13px] text-main-400">{RENDER_FAILED}</p>;

  return (
    <ScrollArea
      ref={scrollerRef}
      orientation="both"
      className="min-h-0 flex-1 bg-main-850"
      onScroll={(event) => {
        const scroller = event.currentTarget;
        setFrame((value) => ({ ...value, top: scroller.scrollTop }));
        viewer.reportPage(pageAt(layout, scroller.scrollTop, scroller.clientHeight));
      }}
    >
      {pdf === null ? (
        <p role="status" className="px-6 pt-6 text-center text-[12.5px] text-main-500">
          {RENDERING}
        </p>
      ) : (
        <div
          ref={rootRef}
          className="relative mx-auto"
          style={{
            height: `${String(layout.height)}px`,
            minWidth: `${String(layout.width)}px`,
          }}
        >
          {visible.map((page) => {
            const slot = layout.slots[page - 1]!;
            return (
              <div
                key={page}
                className="absolute inset-x-0"
                style={{ top: `${String(slot.top)}px`, height: `${String(slot.height)}px` }}
              >
                <PdfPageView
                  pdf={pdf}
                  page={page}
                  scale={scale}
                  width={slot.width}
                  height={slot.height}
                  onLayer={onLayer}
                />
              </div>
            );
          })}
        </div>
      )}
    </ScrollArea>
  );
}

export default observer(PdfView);
