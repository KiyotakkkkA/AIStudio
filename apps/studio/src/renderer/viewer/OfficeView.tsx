import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import type ViewerStore from "../stores/ViewerStore";
import GridView from "./GridView";
import { readSlides, readWorkbook, type Sheet, type Slide } from "./ooxml";
import { PAGE_ATTRIBUTE, useDocumentText } from "./useDocumentText";
import { EMPTY_DOCUMENT, RENDERING, RENDER_FAILED, SHEETS_LABEL, SLIDE } from "./viewerCopy";
import { PAGE_PADDING, clampZoom } from "./viewerModel";

type Loaded<T> =
  | { readonly state: "loading" }
  | { readonly state: "failed" }
  | { readonly state: "ready"; readonly value: T };

function useLoaded<T>(load: () => Promise<T>, key: unknown): Loaded<T> {
  const [loaded, setLoaded] = useState<Loaded<T>>({ state: "loading" });
  useEffect(() => {
    let cancelled = false;
    setLoaded({ state: "loading" });
    load()
      .then((value) => {
        if (!cancelled) setLoaded({ state: "ready", value });
      })
      .catch(() => {
        if (!cancelled) setLoaded({ state: "failed" });
      });
    return () => {
      cancelled = true;
    };
  }, [key]);
  return loaded;
}

function Status({ loaded }: { readonly loaded: Loaded<unknown> }) {
  if (loaded.state === "failed")
    return <p className="m-auto px-6 text-center text-[13px] text-main-400">{RENDER_FAILED}</p>;
  return (
    <p role="status" className="px-6 pt-6 text-center text-[12.5px] text-main-500">
      {RENDERING}
    </p>
  );
}

function useScrolledPage(
  viewer: ViewerStore,
  scroller: RefObject<HTMLElement | null>,
  pages: () => HTMLElement[],
  version: number,
): void {
  const request = viewer.pageRequest;
  useEffect(() => {
    const element = scroller.current;
    const target = request === null ? undefined : pages()[request.page - 1];
    if (element === null || target === undefined) return;
    element.scrollTo({
      top:
        target.getBoundingClientRect().top -
        element.getBoundingClientRect().top +
        element.scrollTop -
        12,
    });
  }, [request, version]);
  useEffect(() => {
    const element = scroller.current;
    if (element === null) return;
    const update = () => {
      const top = element.getBoundingClientRect().top + Math.min(element.clientHeight / 3, 200);
      const list = pages();
      let current = 1;
      list.forEach((page, index) => {
        if (page.getBoundingClientRect().top <= top) current = index + 1;
      });
      viewer.reportPage(current);
    };
    element.addEventListener("scroll", update, { passive: true });
    return () => element.removeEventListener("scroll", update);
  }, [viewer, version]);
}

export const DocxView = observer(function DocxView({
  viewer,
  bytes,
}: {
  readonly viewer: ViewerStore;
  readonly bytes: Uint8Array;
}) {
  const scrollerRef = useRef<HTMLDivElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const [version, setVersion] = useState(0);
  const [failed, setFailed] = useState(false);
  const [pageWidth, setPageWidth] = useState(0);
  const [frameWidth, setFrameWidth] = useState(0);

  useEffect(() => {
    const body = bodyRef.current;
    if (body === null) return;
    let cancelled = false;
    let sheet: CSSStyleSheet | null = null;
    void (async () => {
      const { renderAsync } = await import("docx-preview");
      const styles = document.createElement("div");
      await renderAsync(bytes, body, styles, {
        className: "docx",
        inWrapper: true,
        breakPages: true,
        ignoreFonts: true,
        renderAltChunks: false,
        renderComments: false,
        renderChanges: false,
        experimental: false,
      });
      if (cancelled) return;
      sheet = new CSSStyleSheet();
      sheet.replaceSync(
        Array.from(styles.querySelectorAll("style"))
          .map((style) => style.textContent ?? "")
          .join("\n"),
      );
      document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
      const sections = Array.from(body.querySelectorAll<HTMLElement>("section.docx"));
      sections.forEach((section, index) => section.setAttribute(PAGE_ATTRIBUTE, String(index + 1)));
      viewer.setPageCount(Math.max(1, sections.length));
      setPageWidth(sections[0]?.offsetWidth ?? 0);
      setVersion((value) => value + 1);
    })().catch(() => {
      if (!cancelled) setFailed(true);
    });
    return () => {
      cancelled = true;
      if (sheet !== null) {
        const added = sheet;
        document.adoptedStyleSheets = document.adoptedStyleSheets.filter((item) => item !== added);
      }
      body.replaceChildren();
    };
  }, [bytes, viewer]);

  useLayoutEffect(() => {
    const scroller = scrollerRef.current;
    if (scroller === null) return;
    const measure = () => setFrameWidth(scroller.clientWidth);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(scroller);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (viewer.fitWidth && pageWidth > 0 && frameWidth > 0)
      viewer.applyFitZoom(clampZoom((frameWidth - PAGE_PADDING * 2) / pageWidth));
  }, [viewer, viewer.fitWidth, pageWidth, frameWidth]);

  useDocumentText(viewer, bodyRef, scrollerRef, version);
  useScrolledPage(
    viewer,
    scrollerRef,
    () => Array.from(bodyRef.current?.querySelectorAll<HTMLElement>("section.docx") ?? []),
    version,
  );

  return (
    <ScrollArea
      ref={scrollerRef}
      orientation="both"
      className="zvs-docx min-h-0 flex-1 bg-main-850"
    >
      {failed ? (
        <p className="px-6 pt-6 text-center text-[13px] text-main-400">{RENDER_FAILED}</p>
      ) : version === 0 ? (
        <p role="status" className="px-6 pt-6 text-center text-[12.5px] text-main-500">
          {RENDERING}
        </p>
      ) : null}
      <div ref={bodyRef} style={{ zoom: viewer.zoom }} />
    </ScrollArea>
  );
});

export const SheetView = observer(function SheetView({
  viewer,
  bytes,
}: {
  readonly viewer: ViewerStore;
  readonly bytes: Uint8Array;
}) {
  const loaded = useLoaded<Sheet[]>(() => readWorkbook(bytes), bytes);
  const count = loaded.state === "ready" ? loaded.value.length : 0;
  useEffect(() => {
    if (count > 0) viewer.setPageCount(count);
  }, [viewer, count]);
  if (loaded.state !== "ready") return <Status loaded={loaded} />;
  const sheets = loaded.value;
  if (sheets.length === 0)
    return <p className="m-auto px-6 text-center text-[13px] text-main-400">{EMPTY_DOCUMENT}</p>;
  const index = Math.min(sheets.length, Math.max(1, viewer.page)) - 1;
  const sheet = sheets[index]!;
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div
        role="tablist"
        aria-label={SHEETS_LABEL}
        className="flex flex-none gap-1 overflow-x-auto border-b border-line bg-main-900 px-3 py-1.5"
      >
        {sheets.map((item, position) => (
          <button
            key={`${item.name}:${String(position)}`}
            type="button"
            role="tab"
            aria-selected={position === index}
            className={`h-7 flex-none rounded-control px-2.5 text-[12px] font-medium ${position === index ? "bg-main-700 text-main-50" : "text-main-400 hover:bg-main-800 hover:text-main-100"}`}
            onClick={() => viewer.requestPage(position + 1)}
          >
            {item.name}
          </button>
        ))}
      </div>
      <GridView key={index} viewer={viewer} rows={sheet.rows} page={index + 1} />
    </div>
  );
});

export const SlidesView = observer(function SlidesView({
  viewer,
  bytes,
}: {
  readonly viewer: ViewerStore;
  readonly bytes: Uint8Array;
}) {
  const scrollerRef = useRef<HTMLDivElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const loaded = useLoaded<Slide[]>(() => readSlides(bytes), bytes);
  const slides = loaded.state === "ready" ? loaded.value : [];
  const version = loaded.state === "ready" ? 1 : 0;
  useEffect(() => {
    if (slides.length > 0) viewer.setPageCount(slides.length);
  }, [viewer, slides.length]);
  useDocumentText(viewer, rootRef, scrollerRef, version);
  useScrolledPage(
    viewer,
    scrollerRef,
    () => Array.from(rootRef.current?.querySelectorAll<HTMLElement>(`[${PAGE_ATTRIBUTE}]`) ?? []),
    version,
  );
  if (loaded.state !== "ready") return <Status loaded={loaded} />;
  return (
    <ScrollArea
      ref={scrollerRef}
      orientation="both"
      className="min-h-0 flex-1 bg-main-850 px-5 py-5"
    >
      <div
        ref={rootRef}
        className="mx-auto flex max-w-180 flex-col gap-4"
        style={{ fontSize: `${String(Math.round(viewer.zoom * 100))}%` }}
      >
        {slides.length === 0 ? (
          <p className="text-center text-[13px] text-main-400">{EMPTY_DOCUMENT}</p>
        ) : (
          slides.map((slide) => (
            <article
              key={slide.index}
              {...{ [PAGE_ATTRIBUTE]: slide.index }}
              className="flex aspect-video flex-col gap-2 overflow-hidden rounded-card border border-line bg-main-900 p-6 shadow-card"
            >
              <span className="text-[10.5px] font-medium tracking-[0.12em] text-main-500 uppercase select-none">
                {SLIDE} {slide.index.toLocaleString("ru-RU")}
              </span>
              {slide.paragraphs.map((paragraph, index) => (
                <p
                  key={index}
                  className={`m-0 text-main-100 ${index === 0 ? "text-[1.25em] font-semibold" : "text-[1em] leading-relaxed"}`}
                >
                  {paragraph}
                </p>
              ))}
            </article>
          ))
        )}
      </div>
    </ScrollArea>
  );
});
