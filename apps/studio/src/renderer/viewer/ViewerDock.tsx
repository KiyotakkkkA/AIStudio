import { useEffect, useRef, type KeyboardEvent, type PointerEvent } from "react";
import { observer } from "mobx-react-lite";
import {
  mdiChevronDown,
  mdiChevronUp,
  mdiClose,
  mdiFileAlertOutline,
  mdiFileCancelOutline,
  mdiFileQuestionOutline,
  mdiMessageTextOutline,
} from "@mdi/js";
import Button from "../ui/atoms/buttons/Button";
import IconButton from "../ui/atoms/buttons/IconButton";
import Icon from "../ui/atoms/Icon";
import EmptyState from "../ui/molecules/EmptyState";
import { useStore } from "../stores/useStore";
import type ViewerStore from "../stores/ViewerStore";
import ViewerHeader from "./ViewerHeader";
import PdfView from "./PdfView";
import TextView from "./TextView";
import ImageView from "./ImageView";
import { DocxView, SheetView, SlidesView } from "./OfficeView";
import {
  ASK_ABOUT,
  CHIP_APPROXIMATE,
  CHIP_APPROXIMATE_HINT,
  CLEAR_SELECTION,
  LOADING,
  PPTX_NOTE,
  RESIZE_VIEWER,
  SEARCH_CLOSE,
  SEARCH_EMPTY,
  SEARCH_NEXT,
  SEARCH_PLACEHOLDER,
  SEARCH_PREVIOUS,
  VIEWER_LABEL,
  documentFacts,
  matchesLabel,
  selectionLabel,
} from "./viewerCopy";
import { THREAD_MIN_WIDTH } from "./viewerModel";
import { DocumentSkeleton } from "../ui/molecules/LoadingSkeleton";

const NARROW = "(max-width: 1250px)";

const PROBLEM_ICONS = {
  missing: mdiFileQuestionOutline,
  unsupported: mdiFileCancelOutline,
  forbidden: mdiFileCancelOutline,
  tooLarge: mdiFileAlertOutline,
  failed: mdiFileAlertOutline,
} as const;

const SearchBar = observer(function SearchBar({ viewer }: { readonly viewer: ViewerStore }) {
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);
  const empty = viewer.query.trim().length > 0 && viewer.matchCount === 0;
  return (
    <div className="flex h-11 flex-none items-center gap-1.5 border-b border-line bg-main-900 px-3">
      <input
        ref={input}
        type="search"
        aria-label={SEARCH_PLACEHOLDER}
        placeholder={SEARCH_PLACEHOLDER}
        value={viewer.query}
        onChange={(event) => viewer.setQuery(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            if (event.shiftKey) viewer.previousMatch();
            else viewer.nextMatch();
          }
          if (event.key === "Escape") {
            event.stopPropagation();
            viewer.closeSearch();
          }
        }}
        className="h-7 min-w-0 flex-1 rounded-control border border-line bg-main-850 px-2.5 text-[12.5px] text-main-100 outline-none placeholder:text-main-500 focus:border-accent-line"
      />
      <span
        role="status"
        className={`flex-none font-mono text-[11px] tabular-nums ${empty ? "text-warn" : "text-main-500"}`}
      >
        {viewer.query.trim().length === 0
          ? null
          : empty
            ? SEARCH_EMPTY
            : matchesLabel(viewer.matchIndex + 1, viewer.matchCount)}
      </span>
      <IconButton
        label={SEARCH_PREVIOUS}
        tooltip="bottom-center"
        path={mdiChevronUp}
        iconSize={17}
        disabled={viewer.matchCount === 0}
        onClick={viewer.previousMatch}
      />
      <IconButton
        label={SEARCH_NEXT}
        tooltip="bottom-center"
        path={mdiChevronDown}
        iconSize={17}
        disabled={viewer.matchCount === 0}
        onClick={viewer.nextMatch}
      />
      <IconButton
        label={SEARCH_CLOSE}
        tooltip="bottom-left"
        path={mdiClose}
        iconSize={16}
        onClick={viewer.closeSearch}
      />
    </div>
  );
});

const ViewerBody = observer(function ViewerBody({ viewer }: { readonly viewer: ViewerStore }) {
  if (viewer.status === "failed" && viewer.problem !== null) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <EmptyState
          icon={PROBLEM_ICONS[viewer.problem.kind]}
          title={viewer.problem.title}
          description={viewer.problem.hint}
        />
      </div>
    );
  }
  const content = viewer.content;
  const document = viewer.document;
  if (content === null || document === null) {
    return <DocumentSkeleton label={LOADING} paragraphs={4} className="px-6 pt-6" />;
  }
  switch (content.type) {
    case "pdf":
      return <PdfView viewer={viewer} bytes={content.bytes} />;
    case "image":
      return <ImageView viewer={viewer} url={content.url} name={document.name} />;
    case "text":
      return (
        <TextView
          viewer={viewer}
          text={content.text}
          kind={content.kind}
          language={content.language}
          mimeType={document.mimeType}
        />
      );
    case "docx":
      return <DocxView viewer={viewer} bytes={content.bytes} />;
    case "xlsx":
      return <SheetView viewer={viewer} bytes={content.bytes} />;
    case "pptx":
      return <SlidesView viewer={viewer} bytes={content.bytes} />;
  }
});

const ViewerFooter = observer(function ViewerFooter({
  viewer,
  onAsk,
}: {
  readonly viewer: ViewerStore;
  readonly onAsk: () => void;
}) {
  const document = viewer.document;
  if (document === null) return null;
  const office = document.officeFormat !== undefined;
  return (
    <div className="flex flex-none flex-col border-t border-line">
      {document.officeFormat === "pptx" ? (
        <p className="m-0 border-b border-line bg-main-850 px-4 py-2 text-[12px] leading-snug text-main-400">
          {PPTX_NOTE}
        </p>
      ) : null}
      {viewer.actionError !== null || viewer.notice !== null ? (
        <p
          role={viewer.actionError === null ? "status" : "alert"}
          className={`m-0 px-4 pt-2 text-[12px] ${viewer.actionError === null ? "text-ok" : "text-err"}`}
        >
          {viewer.actionError ?? viewer.notice}
        </p>
      ) : null}
      <div className="flex min-h-12 flex-wrap items-center gap-2 px-3 py-2">
        {office ? (
          <span
            title={CHIP_APPROXIMATE_HINT}
            className="inline-flex h-6 items-center rounded-pill bg-warn-soft px-2 text-[11px] font-medium text-warn"
          >
            {CHIP_APPROXIMATE}
          </span>
        ) : null}
        <span className="inline-flex h-6 items-center rounded-pill bg-main-800 px-2 font-mono text-[11px] text-main-400">
          {documentFacts(viewer.pageCount, document.sizeBytes)}
        </span>
        <span className="min-w-0 flex-1" />
        {viewer.selection === null ? null : (
          <>
            <span className="text-[12px] text-main-500">
              {selectionLabel(viewer.selection.text.length)}
            </span>
            <Button tone="ghost" onClick={() => viewer.select(null)}>
              {CLEAR_SELECTION}
            </Button>
          </>
        )}
        <Button tone="primary" disabled={!viewer.canAsk} onClick={onAsk}>
          <Icon path={mdiMessageTextOutline} size={15} />
          {ASK_ABOUT}
        </Button>
      </div>
    </div>
  );
});

function ViewerDock() {
  const { viewer } = useStore();
  const sectionRef = useRef<HTMLElement>(null);
  if (!viewer.isOpen) return null;

  const startResize = (event: PointerEvent<HTMLDivElement>) => {
    const handle = event.currentTarget;
    const view = sectionRef.current;
    if (view === null) return;
    handle.setPointerCapture(event.pointerId);
    const right = view.getBoundingClientRect().right;
    const thread = view.previousElementSibling?.getBoundingClientRect();
    const limit = thread === undefined ? Infinity : right - thread.left - THREAD_MIN_WIDTH;
    const move = (next: globalThis.PointerEvent) => {
      viewer.setDockWidth(Math.min(limit, right - next.clientX));
    };
    const stop = () => {
      viewer.commitDockWidth();
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
  };

  const keyDown = (event: KeyboardEvent<HTMLElement>) => {
    const target = event.target as HTMLElement;
    if (target.closest("input, textarea, select, [role=menu]") !== null) return;
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
      event.preventDefault();
      viewer.openSearch();
      return;
    }
    if (event.key === "Escape") {
      if (viewer.searchOpen) viewer.closeSearch();
      else if (viewer.expanded) viewer.toggleExpanded();
      return;
    }
    if (event.key === "PageDown" || (event.key === "ArrowRight" && event.altKey)) {
      event.preventDefault();
      viewer.nextPage();
    }
    if (event.key === "PageUp" || (event.key === "ArrowLeft" && event.altKey)) {
      event.preventDefault();
      viewer.previousPage();
    }
    if (event.key === "Home" && event.ctrlKey) viewer.requestPage(1);
    if (event.key === "End" && event.ctrlKey && viewer.pageCount !== null)
      viewer.requestPage(viewer.pageCount);
  };

  const ask = () => {
    if (viewer.ask() && window.matchMedia(NARROW).matches) viewer.close();
  };

  return (
    <section
      ref={sectionRef}
      aria-label={VIEWER_LABEL}
      tabIndex={-1}
      onKeyDown={keyDown}
      style={viewer.expanded ? undefined : { width: `${String(viewer.dockWidth)}px` }}
      className={`@container flex min-h-0 min-w-0 flex-col bg-main-900 outline-none ${
        viewer.expanded
          ? "fixed inset-0 z-40"
          : "relative flex-none border-l border-line max-[1250px]:absolute max-[1250px]:inset-0 max-[1250px]:z-20 max-[1250px]:w-auto! max-[1250px]:border-l-0"
      }`}
    >
      {viewer.expanded ? null : (
        <div
          role="separator"
          aria-orientation="vertical"
          aria-label={RESIZE_VIEWER}
          onPointerDown={startResize}
          className="absolute inset-y-0 -left-1 z-10 w-2 cursor-col-resize hover:bg-accent-line max-[1250px]:hidden"
        />
      )}
      <ViewerHeader viewer={viewer} onBack={viewer.close} />
      {viewer.searchOpen ? <SearchBar viewer={viewer} /> : null}
      <div className="flex min-h-0 flex-1 flex-col">
        <ViewerBody viewer={viewer} />
      </div>
      <ViewerFooter viewer={viewer} onAsk={ask} />
    </section>
  );
}

export default observer(ViewerDock);
