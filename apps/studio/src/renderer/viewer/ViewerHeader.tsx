import { useEffect, useState } from "react";
import { observer } from "mobx-react-lite";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import {
  mdiArrowCollapse,
  mdiArrowExpand,
  mdiArrowLeft,
  mdiChevronDown,
  mdiChevronLeft,
  mdiChevronRight,
  mdiClose,
  mdiFileDocumentOutline,
  mdiFileExcelOutline,
  mdiFileImageOutline,
  mdiFilePdfBox,
  mdiFilePowerpointOutline,
  mdiFileWordOutline,
  mdiFolderOpenOutline,
  mdiMagnify,
  mdiTrayArrowDown,
} from "@mdi/js";
import IconButton from "../ui/atoms/buttons/IconButton";
import Icon from "../ui/atoms/Icon";
import type ViewerStore from "../stores/ViewerStore";
import {
  BACK_TO_CHAT,
  CLOSE,
  COLLAPSE,
  DOWNLOAD,
  EXPAND,
  FIT_WIDTH,
  NEXT_PAGE,
  PAGE_INPUT,
  PREVIOUS_PAGE,
  REVEAL,
  SEARCH,
  ZOOM_IN,
  ZOOM_LABEL,
  ZOOM_OUT,
  zoomLabel,
} from "./viewerCopy";
import { ZOOM_PRESETS, extensionOf } from "./viewerModel";

export function documentIcon(name: string): string {
  switch (extensionOf(name)) {
    case "pdf":
      return mdiFilePdfBox;
    case "docx":
      return mdiFileWordOutline;
    case "xlsx":
    case "csv":
    case "tsv":
      return mdiFileExcelOutline;
    case "pptx":
      return mdiFilePowerpointOutline;
    case "png":
    case "jpg":
    case "jpeg":
    case "jfif":
    case "gif":
    case "webp":
    case "bmp":
      return mdiFileImageOutline;
    default:
      return mdiFileDocumentOutline;
  }
}

const PageControl = observer(function PageControl({ viewer }: { readonly viewer: ViewerStore }) {
  const [draft, setDraft] = useState(String(viewer.page));
  useEffect(() => setDraft(String(viewer.page)), [viewer.page]);
  const count = viewer.pageCount;
  if (count === null || count <= 1) return null;
  const commit = () => {
    const value = Number(draft);
    if (Number.isFinite(value)) viewer.requestPage(value);
    else setDraft(String(viewer.page));
  };
  return (
    <div className="flex flex-none items-center gap-0.5">
      <IconButton
        label={PREVIOUS_PAGE}
        tooltip="bottom-center"
        path={mdiChevronLeft}
        iconSize={17}
        disabled={viewer.page <= 1}
        onClick={viewer.previousPage}
      />
      <span className="flex items-center gap-1 font-mono text-[11.5px] text-main-300 tabular-nums">
        <input
          aria-label={PAGE_INPUT}
          inputMode="numeric"
          value={draft}
          onChange={(event) => setDraft(event.target.value.replace(/\D/gu, ""))}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") commit();
          }}
          className="h-6 w-9 rounded-pill border border-line bg-main-900 px-1 text-center text-main-100 outline-none focus:border-accent-line"
        />
        <span className="text-main-500">/ {count.toLocaleString("ru-RU")}</span>
      </span>
      <IconButton
        label={NEXT_PAGE}
        tooltip="bottom-center"
        path={mdiChevronRight}
        iconSize={17}
        disabled={viewer.page >= count}
        onClick={viewer.nextPage}
      />
    </div>
  );
});

const ZoomControl = observer(function ZoomControl({ viewer }: { readonly viewer: ViewerStore }) {
  return (
    <Dropdown menuWidth={168} menuPlacement="bottom-right">
      <Dropdown.Trigger
        aria-label={ZOOM_LABEL}
        rounded="rounded-lg"
        icon={<Icon path={mdiChevronDown} size={14} className="text-main-500" />}
        className="h-7 flex-none gap-1 border-line bg-transparent px-2 font-mono text-[11.5px] text-main-300 hover:bg-main-700"
      >
        {viewer.fitWidth ? FIT_WIDTH : zoomLabel(viewer.zoom)}
      </Dropdown.Trigger>
      <Dropdown.Menu
        role="menu"
        aria-label={ZOOM_LABEL}
        rounded="rounded-lg"
        className="rounded-card border-line bg-main-900 p-1 shadow-pop"
      >
        <Dropdown.Item
          role="menuitemradio"
          aria-checked={viewer.fitWidth}
          rounded="rounded-lg"
          active={viewer.fitWidth}
          className="text-[13px] text-main-100"
          onClick={viewer.setFitWidth}
        >
          {FIT_WIDTH}
        </Dropdown.Item>
        {ZOOM_PRESETS.map((preset) => {
          const active = !viewer.fitWidth && Math.abs(viewer.zoom - preset) < 0.001;
          return (
            <Dropdown.Item
              key={preset}
              role="menuitemradio"
              aria-checked={active}
              rounded="rounded-lg"
              active={active}
              className="font-mono text-[12.5px] text-main-100"
              onClick={() => viewer.setZoom(preset)}
            >
              {zoomLabel(preset)}
            </Dropdown.Item>
          );
        })}
        <div className="my-1 h-px bg-line" />
        <div className="flex gap-1 px-1 pb-0.5">
          <button
            type="button"
            className="h-7 flex-1 rounded-control text-[12px] text-main-300 hover:bg-main-700 hover:text-main-50"
            onClick={viewer.zoomOut}
          >
            {ZOOM_OUT}
          </button>
          <button
            type="button"
            className="h-7 flex-1 rounded-control text-[12px] text-main-300 hover:bg-main-700 hover:text-main-50"
            onClick={viewer.zoomIn}
          >
            {ZOOM_IN}
          </button>
        </div>
      </Dropdown.Menu>
    </Dropdown>
  );
});

function ViewerHeader({
  viewer,
  onBack,
}: {
  readonly viewer: ViewerStore;
  readonly onBack: () => void;
}) {
  const ready = viewer.document !== null;
  return (
    <div className="flex h-14 flex-none items-center gap-1.5 border-b border-line pr-2 pl-3">
      <span className="min-[1251px]:hidden">
        <IconButton
          label={BACK_TO_CHAT}
          tooltip="bottom-left"
          path={mdiArrowLeft}
          iconSize={17}
          onClick={onBack}
        />
      </span>
      <span className="flex size-8 flex-none items-center justify-center rounded-control bg-main-800 text-main-300">
        <Icon path={documentIcon(viewer.name)} size={17} />
      </span>
      <div className="flex min-w-0 flex-1 flex-col pl-1 leading-tight">
        <span
          className="truncate text-[13px] font-medium text-main-50"
          title={viewer.document?.sourcePath ?? viewer.name}
        >
          {viewer.name}
        </span>
        {viewer.target?.location ? (
          <span className="truncate text-[11px] text-main-500">{viewer.target.location}</span>
        ) : viewer.document ? (
          <span
            className="truncate font-mono text-[10.5px] text-main-500"
            title={viewer.document.sourcePath}
          >
            {viewer.document.sourcePath}
          </span>
        ) : null}
      </div>
      <PageControl viewer={viewer} />
      {ready && viewer.content !== null ? <ZoomControl viewer={viewer} /> : null}
      <span className="mx-0.5 h-5 w-px flex-none bg-line @max-[520px]:hidden" />
      <IconButton
        label={SEARCH}
        tooltip="bottom-center"
        path={mdiMagnify}
        iconSize={17}
        disabled={viewer.status !== "ready" || viewer.content?.type === "image"}
        aria-pressed={viewer.searchOpen}
        className={viewer.searchOpen ? "bg-main-700 text-main-50" : ""}
        onClick={viewer.searchOpen ? viewer.closeSearch : viewer.openSearch}
      />
      <span className="@max-[520px]:hidden">
        <IconButton
          label={DOWNLOAD}
          tooltip="bottom-center"
          path={mdiTrayArrowDown}
          iconSize={17}
          disabled={!ready || viewer.saving}
          onClick={() => void viewer.save()}
        />
      </span>
      <span className="@max-[600px]:hidden">
        <IconButton
          label={REVEAL}
          tooltip="bottom-center"
          path={mdiFolderOpenOutline}
          iconSize={17}
          disabled={!ready}
          onClick={() => void viewer.reveal()}
        />
      </span>
      <span className="max-[1250px]:hidden">
        <IconButton
          label={viewer.expanded ? COLLAPSE : EXPAND}
          tooltip="bottom-center"
          path={viewer.expanded ? mdiArrowCollapse : mdiArrowExpand}
          iconSize={16}
          onClick={viewer.toggleExpanded}
        />
      </span>
      <IconButton
        label={CLOSE}
        tooltip="bottom-left"
        path={mdiClose}
        iconSize={17}
        onClick={viewer.close}
      />
    </div>
  );
}

export default observer(ViewerHeader);
