import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import { mdiClose, mdiPencilOutline, mdiStar, mdiWeb } from "@mdi/js";
import type { BrowserBookmarkDto } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import TextInput from "../../ui/atoms/TextInput";
import Toggle from "../../ui/atoms/Toggle";
import { useStore } from "../../stores/useStore";
import BrowserSubPage from "./BrowserSubPage";
import {
  BOOKMARK_CANCEL,
  BOOKMARK_REMOVE,
  BOOKMARK_RENAME,
  BOOKMARK_RENAME_LABEL,
  BOOKMARK_ROOT,
  BOOKMARK_SAVE,
  BOOKMARKS_EMPTY,
  BOOKMARKS_HINT,
  BOOKMARKS_LABEL,
  HISTORY_CLEAR,
  HISTORY_CLEAR_CONSEQUENCE,
  HISTORY_CLEAR_TITLE,
  HISTORY_EMPTY,
  HISTORY_HINT,
  HISTORY_IMPORTED,
  HISTORY_LABEL,
  HISTORY_NOTHING_FOUND,
  HISTORY_REMOVE,
  HISTORY_SEARCH,
  IMPORT_BOOKMARKS,
  IMPORT_DEFAULT_PROFILE,
  IMPORT_EMPTY,
  IMPORT_HINT,
  IMPORT_HISTORY,
  IMPORT_KIND_LABELS,
  IMPORT_LOADING,
  IMPORT_RUN,
  IMPORT_RUNNING,
  IMPORT_TITLE,
  historyCountLabel,
  visitTime,
} from "./browserLibraryCopy";
import { siteHost } from "./browserSitesCopy";

const CARD = "rounded-card border border-line bg-main-850";
const ROW = "flex items-center gap-3 border-b border-line px-4 py-2.5 last:border-b-0";
const EMPTY = "m-0 px-4 py-6 text-center text-[12.5px] text-main-500";

export const HistoryPage = observer(function HistoryPage({
  onBack,
}: {
  readonly onBack: () => void;
}) {
  const { browserLibrary: library } = useStore();
  useEffect(() => {
    void library.loadHistory();
  }, [library]);
  const searching = library.query.trim().length > 0;
  return (
    <BrowserSubPage
      title={HISTORY_LABEL}
      hint={HISTORY_HINT}
      error={library.error}
      onDismissError={library.dismissError}
      onBack={onBack}
      actions={
        <Button
          tone="secondary"
          size="sm"
          disabled={library.historyTotal === 0 || library.busy}
          needConfirm
          modalSetup={{
            title: HISTORY_CLEAR_TITLE,
            content: HISTORY_CLEAR_CONSEQUENCE,
            confirmLabel: HISTORY_CLEAR,
          }}
          onClick={() => void library.clearHistory()}
        >
          {HISTORY_CLEAR}
        </Button>
      }
    >
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <TextInput
            aria-label={HISTORY_SEARCH}
            placeholder={HISTORY_SEARCH}
            value={library.query}
            onChange={(event) => library.setQuery(event.target.value)}
          />
        </div>
        {library.historyLoaded && library.historyTotal > 0 ? (
          <span className="flex-none text-[12px] text-main-500">
            {historyCountLabel(library.history.length, library.historyTotal)}
          </span>
        ) : null}
      </div>
      <div className={CARD}>
        {library.history.length === 0 ? (
          <p className={EMPTY}>
            {library.historyLoaded ? (searching ? HISTORY_NOTHING_FOUND : HISTORY_EMPTY) : null}
          </p>
        ) : (
          <ul className="m-0 list-none p-0">
            {library.history.map((entry) => (
              <li key={entry.id} className={ROW}>
                <Icon path={mdiWeb} size={16} className="flex-none text-main-500" />
                <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="truncate text-[13px] text-main-100" title={entry.url}>
                    {entry.title.length > 0 ? entry.title : siteHost(entry.url)}
                  </span>
                  <span className="truncate font-mono text-[11px] text-main-500">{entry.url}</span>
                </div>
                {entry.importedFrom === null ? null : (
                  <Chip>
                    {HISTORY_IMPORTED} · {IMPORT_KIND_LABELS[entry.importedFrom]}
                  </Chip>
                )}
                <span className="flex-none text-[11.5px] text-main-500 tabular-nums">
                  {visitTime(entry.visitedAt)}
                </span>
                <IconButton
                  path={mdiClose}
                  label={HISTORY_REMOVE}
                  size="sm"
                  disabled={library.busy}
                  onClick={() => void library.removeVisit(entry.id)}
                />
              </li>
            ))}
          </ul>
        )}
      </div>
    </BrowserSubPage>
  );
});

function groupByFolder(bookmarks: readonly BrowserBookmarkDto[]): [string, BrowserBookmarkDto[]][] {
  const groups = new Map<string, BrowserBookmarkDto[]>();
  for (const bookmark of bookmarks)
    groups.set(bookmark.folder, [...(groups.get(bookmark.folder) ?? []), bookmark]);
  return [...groups];
}

const BookmarkRow = observer(function BookmarkRow({
  bookmark,
}: {
  readonly bookmark: BrowserBookmarkDto;
}) {
  const { browserLibrary: library } = useStore();
  if (library.renamingId === bookmark.id)
    return (
      <li className={ROW}>
        <form
          className="flex min-w-0 flex-1 items-center gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            void library.saveRename();
          }}
        >
          <div className="min-w-0 flex-1">
            <TextInput
              aria-label={BOOKMARK_RENAME_LABEL}
              autoFocus
              value={library.renameDraft}
              onChange={(event) => library.setRenameDraft(event.target.value)}
            />
          </div>
          <Button tone="ghost" size="sm" onClick={library.cancelRename}>
            {BOOKMARK_CANCEL}
          </Button>
          <Button type="submit" tone="primary" size="sm" disabled={!library.canRename}>
            {BOOKMARK_SAVE}
          </Button>
        </form>
      </li>
    );
  return (
    <li className={ROW}>
      <Icon path={mdiStar} size={15} className="flex-none text-accent-medium" />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="truncate text-[13px] text-main-100">{bookmark.title}</span>
        <span className="truncate font-mono text-[11px] text-main-500">{bookmark.url}</span>
      </div>
      <IconButton
        path={mdiPencilOutline}
        label={BOOKMARK_RENAME}
        size="sm"
        onClick={() => library.startRename(bookmark)}
      />
      <IconButton
        path={mdiClose}
        label={BOOKMARK_REMOVE}
        size="sm"
        disabled={library.busy}
        onClick={() => void library.removeBookmark(bookmark.id)}
      />
    </li>
  );
});

export const BookmarksPage = observer(function BookmarksPage({
  onBack,
}: {
  readonly onBack: () => void;
}) {
  const { browserLibrary: library } = useStore();
  useEffect(() => {
    void library.loadBookmarks();
  }, [library]);
  const groups = groupByFolder(library.bookmarks);
  return (
    <BrowserSubPage
      title={BOOKMARKS_LABEL}
      hint={BOOKMARKS_HINT}
      error={library.error}
      onDismissError={library.dismissError}
      onBack={onBack}
    >
      {groups.length === 0 ? (
        <div className={CARD}>
          <p className={EMPTY}>{library.bookmarksLoaded ? BOOKMARKS_EMPTY : null}</p>
        </div>
      ) : (
        groups.map(([folder, items]) => (
          <section
            key={folder}
            aria-label={folder || BOOKMARK_ROOT}
            className="flex flex-col gap-2"
          >
            <h4 className="m-0 text-[12px] font-medium text-main-400">{folder || BOOKMARK_ROOT}</h4>
            <ul className={`m-0 list-none p-0 ${CARD}`}>
              {items.map((bookmark) => (
                <BookmarkRow key={bookmark.id} bookmark={bookmark} />
              ))}
            </ul>
          </section>
        ))
      )}
    </BrowserSubPage>
  );
});

export const ImportPage = observer(function ImportPage({
  onBack,
}: {
  readonly onBack: () => void;
}) {
  const { browserLibrary: library } = useStore();
  useEffect(() => {
    void library.loadSources();
  }, [library]);
  return (
    <BrowserSubPage
      title={IMPORT_TITLE}
      hint={IMPORT_HINT}
      error={library.error}
      onDismissError={library.dismissError}
      onBack={onBack}
    >
      {library.notice === null ? null : (
        <p role="status" className="m-0 text-[12.5px] text-main-300">
          {library.notice}
        </p>
      )}
      <div className={CARD}>
        {library.sources.length === 0 ? (
          <p className={EMPTY}>{library.sourcesLoaded ? IMPORT_EMPTY : IMPORT_LOADING}</p>
        ) : (
          <ul className="m-0 list-none p-0">
            {library.sources.map((source) => {
              const choice = library.choiceOf(source.id);
              const running = library.importing === source.id;
              return (
                <li key={source.id} className={`${ROW} flex-wrap py-3`}>
                  <div className="flex min-w-48 flex-1 flex-col gap-0.5">
                    <span className="text-[13px] font-medium text-main-100">
                      {IMPORT_KIND_LABELS[source.kind]}
                    </span>
                    <span className="text-[12px] text-main-500">
                      {source.profile.length > 0 ? source.profile : IMPORT_DEFAULT_PROFILE}
                    </span>
                  </div>
                  {source.history ? (
                    <span className="flex items-center">
                      <Toggle
                        label={IMPORT_HISTORY}
                        checked={choice.history}
                        onChange={(value) => library.setChoice(source.id, { history: value })}
                      />
                    </span>
                  ) : null}
                  {source.bookmarks ? (
                    <span className="flex items-center">
                      <Toggle
                        label={IMPORT_BOOKMARKS}
                        checked={choice.bookmarks}
                        onChange={(value) => library.setChoice(source.id, { bookmarks: value })}
                      />
                    </span>
                  ) : null}
                  <Button
                    tone="secondary"
                    size="sm"
                    disabled={library.importing !== null || (!choice.history && !choice.bookmarks)}
                    onClick={() => void library.runImport(source)}
                  >
                    {running ? IMPORT_RUNNING : IMPORT_RUN}
                  </Button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </BrowserSubPage>
  );
});
