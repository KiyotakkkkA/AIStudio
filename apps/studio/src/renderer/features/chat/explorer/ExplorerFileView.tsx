import { useState, type PointerEvent } from "react";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiArrowLeft, mdiClose, mdiFileAlertOutline, mdiFileOutline } from "@mdi/js";
import Icon from "../../../ui/atoms/Icon";
import IconButton from "../../../ui/atoms/buttons/IconButton";
import SegmentedControl from "../../../ui/atoms/SegmentedControl";
import EmptyState from "../../../ui/molecules/EmptyState";
import { formatBytes } from "../../tasks/runPresentation";
import ChatMarkdown from "../ChatMarkdown";
import type ExplorerStore from "./ExplorerStore";
import {
  BACK_TO_CHAT,
  BINARY_HINT,
  BINARY_TITLE,
  CLOSE_FILE,
  FILE_EMPTY,
  FILE_LOADING,
  FILE_REMOVED,
  FILE_REMOVED_HINT,
  FILE_TRUNCATED,
  FILE_VIEW_LABEL,
  MODE_LABEL,
  MODE_PREVIEW,
  MODE_TEXT,
  RESIZE_VIEW,
  isMarkdown,
  modifiedLabel,
} from "./explorerCopy";

type Mode = "preview" | "text";

const MODES = [
  { value: "preview", label: MODE_PREVIEW },
  { value: "text", label: MODE_TEXT },
] as const;

function PlainText({ text }: { readonly text: string }) {
  const lines = text.endsWith("\n") ? text.slice(0, -1).split("\n") : text.split("\n");
  const gutter = lines.map((_, index) => String(index + 1)).join("\n");
  return (
    <ScrollArea orientation="both" className="min-h-0 flex-1">
      <div className="flex min-w-max font-mono text-[12.5px] leading-[1.6]">
        <pre
          aria-hidden="true"
          className="sticky left-0 flex-none bg-main-900 py-3 pr-3 pl-4 text-right text-main-500 select-none"
        >
          {gutter}
        </pre>
        <pre className="flex-1 py-3 pr-6 pl-3 text-main-100">{text}</pre>
      </div>
    </ScrollArea>
  );
}

export default observer(function ExplorerFileView({
  explorer,
}: {
  readonly explorer: ExplorerStore;
}) {
  const [mode, setMode] = useState<Mode>("preview");
  const path = explorer.openPath;
  if (path === null) return null;
  const file = explorer.file;
  const markdown = isMarkdown(path);
  const modified = modifiedLabel(file?.modifiedAt ?? null);

  const startResize = (event: PointerEvent<HTMLDivElement>) => {
    const handle = event.currentTarget;
    const view = handle.parentElement;
    if (view === null) return;
    handle.setPointerCapture(event.pointerId);
    const right = view.getBoundingClientRect().right;
    const thread = view.previousElementSibling?.getBoundingClientRect();
    const limit = thread === undefined ? Infinity : right - thread.left - 420;
    const move = (next: globalThis.PointerEvent) => {
      explorer.setViewWidth(Math.min(limit, right - next.clientX));
    };
    const stop = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
  };

  return (
    <section
      aria-label={FILE_VIEW_LABEL}
      style={{ width: `${String(explorer.viewWidth)}px` }}
      className="relative flex min-h-0 min-w-0 flex-none flex-col border-l border-line bg-main-900 max-[1250px]:absolute max-[1250px]:inset-0 max-[1250px]:z-20 max-[1250px]:w-auto! max-[1250px]:border-l-0"
    >
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label={RESIZE_VIEW}
        onPointerDown={startResize}
        className="absolute inset-y-0 -left-1 z-10 w-2 cursor-col-resize hover:bg-accent-line max-[1250px]:hidden"
      />
      <div className="flex h-14 flex-none items-center gap-2 px-4">
        <span className="min-[1251px]:hidden">
          <IconButton
            label={BACK_TO_CHAT}
            tooltip="bottom-center"
            path={mdiArrowLeft}
            iconSize={17}
            onClick={explorer.closeFile}
          />
        </span>
        <Icon path={mdiFileOutline} size={15} />
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="truncate font-mono text-[12.5px] text-main-100" title={path}>
            {path}
          </span>
          {file === null ? null : (
            <span className="truncate text-[11px] text-main-500">
              {formatBytes(file.size)}
              {modified === null ? null : ` · ${modified}`}
            </span>
          )}
        </div>
        {markdown && file?.text != null ? (
          <SegmentedControl label={MODE_LABEL} value={mode} options={MODES} onChange={setMode} />
        ) : null}
        <IconButton
          label={CLOSE_FILE}
          tooltip="bottom-center"
          path={mdiClose}
          iconSize={17}
          onClick={explorer.closeFile}
        />
      </div>
      {explorer.fileRemoved ? (
        <div
          role="status"
          className="mx-4 mb-2 rounded-control bg-warn-soft px-3 py-2 text-[12px] text-warn"
        >
          {FILE_REMOVED}. {FILE_REMOVED_HINT}
        </div>
      ) : null}
      {file?.truncated === true ? (
        <div className="mx-4 mb-2 rounded-control bg-main-800 px-3 py-2 text-[12px] text-main-400">
          {FILE_TRUNCATED}
        </div>
      ) : null}
      {explorer.fileError === null || file === null ? null : (
        <div
          role="alert"
          className="mx-4 mb-2 rounded-control bg-err-soft px-3 py-2 text-[12px] text-err"
        >
          {explorer.fileError}
        </div>
      )}
      <div
        className={`flex min-h-0 flex-1 flex-col border-t border-line ${explorer.fileRemoved ? "opacity-60" : ""}`}
      >
        {file === null ? (
          explorer.fileLoading ? (
            <p role="status" className="px-4 pt-3 text-[12px] text-main-500">
              {FILE_LOADING}
            </p>
          ) : explorer.fileError !== null ? (
            <div className="flex flex-1 items-center justify-center">
              <EmptyState
                icon={mdiFileAlertOutline}
                title={explorer.fileError}
                description={path}
              />
            </div>
          ) : null
        ) : file.text === null ? (
          <div className="flex flex-1 items-center justify-center">
            <EmptyState
              icon={mdiFileOutline}
              title={BINARY_TITLE}
              description={`${BINARY_HINT} · ${formatBytes(file.size)}`}
            />
          </div>
        ) : file.text.length === 0 ? (
          <p className="px-4 pt-3 text-[12px] text-main-500">{FILE_EMPTY}</p>
        ) : markdown && mode === "preview" ? (
          <ScrollArea className="min-h-0 flex-1 px-6 pt-5 pb-8">
            <ChatMarkdown content={file.text} />
          </ScrollArea>
        ) : (
          <PlainText text={file.text} />
        )}
      </div>
    </section>
  );
});
