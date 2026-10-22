import { useEffect, type KeyboardEvent } from "react";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import {
  mdiChevronRight,
  mdiCollapseAllOutline,
  mdiFileOutline,
  mdiFolderAlertOutline,
  mdiFolderOpenOutline,
  mdiFolderOutline,
  mdiNotificationClearAll,
} from "@mdi/js";
import Button from "../../../ui/atoms/buttons/Button";
import Icon from "../../../ui/atoms/Icon";
import IconButton from "../../../ui/atoms/buttons/IconButton";
import EmptyState from "../../../ui/molecules/EmptyState";
import SidePanelFrame from "../SidePanelFrame";
import type ChatStore from "../ChatStore";
import { RESTORE_FOLDER_ACCESS, folderName } from "../workingFolderCopy";
import type { ExplorerRow } from "./ExplorerStore";
import {
  ACCESS_MISSING,
  ACCESS_MISSING_HINT,
  CLEAR_MARKS,
  COLLAPSE_ALL,
  EMPTY_FOLDER,
  EMPTY_FOLDER_HINT,
  FILES_PANEL,
  FOLDER_GONE,
  FOLDER_GONE_HINT,
  IGNORED_HINT,
  LIST_TRUNCATED,
  LOADING,
  MARK_CREATED,
  MARK_INSIDE,
  MARK_MODIFIED,
  TREE_LABEL,
} from "./explorerCopy";

const MARK_STYLES = {
  created: { className: "bg-ok", label: MARK_CREATED },
  modified: { className: "bg-accent-medium", label: MARK_MODIFIED },
  inside: { className: "bg-main-400", label: MARK_INSIDE },
} as const;

function rowId(path: string): string {
  return `explorer-row-${encodeURIComponent(path)}`;
}

const ExplorerRowView = observer(function ExplorerRowView({
  store,
  row,
}: {
  readonly store: ChatStore;
  readonly row: ExplorerRow;
}) {
  const explorer = store.explorer;
  const { entry, depth, expanded, loading } = row;
  const folder = entry.kind === "folder";
  const mark = explorer.markOf(entry);
  const focused = explorer.focused === entry.path;
  const open = explorer.openPath === entry.path;
  const markStyle = mark === null ? null : MARK_STYLES[mark];
  return (
    <div
      id={rowId(entry.path)}
      role="treeitem"
      aria-level={depth + 1}
      aria-expanded={folder ? expanded : undefined}
      aria-selected={open}
      title={entry.ignored ? `${entry.name} · ${IGNORED_HINT}` : entry.name}
      onClick={() => void explorer.activate(entry)}
      className={`flex h-7 cursor-pointer items-center gap-1.5 rounded-control pr-2 text-[12.5px] select-none ${open ? "bg-main-700 text-main-50" : "text-main-200 hover:bg-main-750"} ${focused ? "outline-1 -outline-offset-1 outline-accent-line" : ""} ${entry.ignored ? "opacity-55" : ""}`}
      style={{ paddingLeft: `${String(6 + depth * 14)}px` }}
    >
      <span className="flex w-3.5 flex-none justify-center text-main-500">
        {folder ? (
          <span className={`transition-transform ${expanded ? "rotate-90" : ""}`}>
            <Icon path={mdiChevronRight} size={14} />
          </span>
        ) : null}
      </span>
      <span className="flex-none text-main-400">
        <Icon
          path={folder ? (expanded ? mdiFolderOpenOutline : mdiFolderOutline) : mdiFileOutline}
          size={15}
        />
      </span>
      <span className="min-w-0 flex-1 truncate">{entry.name}</span>
      {loading ? <span className="text-[11px] text-main-500">{LOADING}</span> : null}
      {markStyle === null ? null : (
        <span
          role="img"
          aria-label={markStyle.label}
          title={markStyle.label}
          className={`size-1.5 flex-none rounded-full ${markStyle.className}`}
        />
      )}
    </div>
  );
});

export default observer(function ChatExplorer({ store }: { readonly store: ChatStore }) {
  const explorer = store.explorer;
  const conversationId = store.active?.id ?? null;
  const folder = store.workingFolder;
  const access = store.workingFolderAccess;

  useEffect(() => {
    if (conversationId === null || folder === null) return;
    void explorer.show(conversationId);
    return () => explorer.hide();
  }, [explorer, conversationId, folder, access]);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const handled = ["ArrowDown", "ArrowUp", "ArrowRight", "ArrowLeft", "Enter", " "];
    if (!handled.includes(event.key)) return;
    event.preventDefault();
    if (event.key === "ArrowDown") explorer.moveFocus(1);
    else if (event.key === "ArrowUp") explorer.moveFocus(-1);
    else if (event.key === "ArrowRight") void explorer.expandFocused();
    else if (event.key === "ArrowLeft") explorer.collapseFocused();
    else {
      const row = explorer.rows.find((candidate) => candidate.entry.path === explorer.focused);
      if (row !== undefined) void explorer.activate(row.entry);
    }
  };

  const rootFolder = explorer.folders.get("");
  const rows = explorer.rows;

  return (
    <SidePanelFrame
      store={store}
      title={folder === null ? FILES_PANEL : folderName(folder)}
      titleHint={folder ?? undefined}
      actions={
        <>
          <IconButton
            label={CLEAR_MARKS}
            tooltip="bottom-right"
            path={mdiNotificationClearAll}
            disabled={!explorer.hasMarks}
            onClick={explorer.clearMarks}
          />
          <IconButton
            label={COLLAPSE_ALL}
            tooltip="bottom-right"
            path={mdiCollapseAllOutline}
            disabled={explorer.expanded.size === 0}
            onClick={explorer.collapseAll}
          />
        </>
      }
    >
      {explorer.root === "missing" ? (
        <div className="flex flex-1 items-center justify-center px-4 pb-10">
          <EmptyState
            icon={mdiFolderAlertOutline}
            title={ACCESS_MISSING}
            description={ACCESS_MISSING_HINT}
            action={
              <Button
                type="button"
                disabled={!store.canChangeFolder}
                onClick={() => void store.restoreFolderAccess()}
              >
                {RESTORE_FOLDER_ACCESS}
              </Button>
            }
          />
        </div>
      ) : explorer.root === "gone" ? (
        <div className="flex flex-1 items-center justify-center px-4 pb-10">
          <EmptyState
            icon={mdiFolderAlertOutline}
            title={FOLDER_GONE}
            description={FOLDER_GONE_HINT}
          />
        </div>
      ) : !explorer.rootLoaded ? (
        <p role="status" className="px-4 pt-1 text-[12px] text-main-500">
          {LOADING}
        </p>
      ) : rows.length === 0 ? (
        <div className="flex flex-1 items-center justify-center px-4 pb-10">
          <EmptyState
            icon={mdiFolderOutline}
            title={EMPTY_FOLDER}
            description={EMPTY_FOLDER_HINT}
          />
        </div>
      ) : (
        <ScrollArea className="min-h-0 flex-1 px-2 pb-4">
          <div
            role="tree"
            aria-label={TREE_LABEL}
            tabIndex={0}
            aria-activedescendant={explorer.focused === null ? undefined : rowId(explorer.focused)}
            onKeyDown={onKeyDown}
            onFocus={() => {
              if (explorer.focused === null) explorer.moveFocus(1);
            }}
            className="flex flex-col gap-px rounded-control focus-visible:outline-none"
          >
            {rows.map((row) => (
              <ExplorerRowView key={row.entry.path} store={store} row={row} />
            ))}
          </div>
          {rootFolder?.truncated === true ? (
            <p className="px-2 pt-2 text-[11.5px] text-main-500">{LIST_TRUNCATED}</p>
          ) : null}
        </ScrollArea>
      )}
      {explorer.error === null ? null : (
        <p
          role="alert"
          className="mx-4 mb-3 rounded-control bg-err-soft px-3 py-2 text-[12px] text-err"
        >
          {explorer.error}
        </p>
      )}
    </SidePanelFrame>
  );
});
