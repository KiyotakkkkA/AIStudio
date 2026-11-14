import { useEffect } from "react";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiNoteAlertOutline, mdiNoteTextOutline, mdiRefresh } from "@mdi/js";
import type { ConversationId } from "@zvs/shared";
import Icon from "../../../ui/atoms/Icon";
import IconButton from "../../../ui/atoms/buttons/IconButton";
import Button from "../../../ui/atoms/buttons/Button";
import Skeleton from "../../../ui/atoms/Skeleton";
import Alert from "../../../ui/molecules/Alert";
import EmptyState from "../../../ui/molecules/EmptyState";
import { LoadingStatus } from "../../../ui/molecules/LoadingSkeleton";
import SidePanelFrame, { type PanelHost } from "../SidePanelFrame";
import type NotesStore from "./NotesStore";
import {
  NOTE_INVALID,
  NOTES_EMPTY,
  NOTES_EMPTY_HINT,
  NOTES_LIST_LABEL,
  NOTES_LOAD_ERROR,
  NOTES_LOAD_HINT,
  NOTES_LOADING,
  NOTES_PANEL,
  NOTES_RETRY,
  noteDate,
} from "./notesCopy";

interface NotesHost extends PanelHost {
  readonly notes: NotesStore;
  readonly active: { readonly id: ConversationId } | null;
}

export default observer(function NotesPanel({ store }: { readonly store: NotesHost }) {
  const notes = store.notes;
  const conversationId = store.active?.id ?? null;

  useEffect(() => {
    if (conversationId === null) return;
    void notes.show(conversationId);
  }, [notes, conversationId]);

  return (
    <SidePanelFrame
      store={store}
      title={NOTES_PANEL}
      actions={
        <IconButton
          label={NOTES_RETRY}
          tooltip="bottom-right"
          path={mdiRefresh}
          disabled={notes.loading || conversationId === null}
          onClick={() => void notes.refresh()}
        />
      }
    >
      {!notes.loaded && notes.error === null ? (
        <LoadingStatus label={NOTES_LOADING} className="flex flex-col gap-3 px-4 pt-1">
          {["w-[62%]", "w-[48%]", "w-[56%]"].map((width) => (
            <div key={width} className="flex flex-col gap-1.5">
              <Skeleton rounded="sm" className={`h-2.5 ${width}`} />
              <Skeleton rounded="sm" className="h-2 w-[30%]" />
            </div>
          ))}
        </LoadingStatus>
      ) : !notes.loaded ? (
        <div className="flex flex-1 items-center justify-center px-4 pb-10">
          <EmptyState
            icon={mdiNoteAlertOutline}
            title={notes.error ?? NOTES_LOAD_ERROR}
            description={NOTES_LOAD_HINT}
            action={<Button onClick={() => void notes.refresh()}>{NOTES_RETRY}</Button>}
          />
        </div>
      ) : notes.notes.length === 0 ? (
        <div className="flex flex-1 items-center justify-center px-4 pb-10">
          <EmptyState
            icon={mdiNoteTextOutline}
            title={NOTES_EMPTY}
            description={NOTES_EMPTY_HINT}
          />
        </div>
      ) : (
        <ScrollArea className="min-h-0 flex-1 px-2 pb-4">
          <ul aria-label={NOTES_LIST_LABEL} className="m-0 flex list-none flex-col gap-px p-0">
            {notes.notes.map((note) => {
              const open = notes.openName === note.name;
              const updated = noteDate(note.updatedAt);
              return (
                <li key={note.name}>
                  <button
                    type="button"
                    title={note.path}
                    aria-current={open ? "true" : undefined}
                    onClick={() => void notes.open(note.name)}
                    className={`flex w-full cursor-pointer items-start gap-2 rounded-control px-2 py-1.5 text-left ${open ? "bg-main-700 text-main-50" : "text-main-200 hover:bg-main-750"}`}
                  >
                    <span
                      className={`mt-0.5 flex-none ${note.error === null ? "text-main-400" : "text-err"}`}
                    >
                      <Icon
                        path={note.error === null ? mdiNoteTextOutline : mdiNoteAlertOutline}
                        size={15}
                      />
                    </span>
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate text-[12.5px]">{note.title ?? note.name}</span>
                      <span className="truncate text-[11px] text-main-500">
                        {note.error === null
                          ? (updated ?? note.name)
                          : `${NOTE_INVALID} · ${note.name}`}
                      </span>
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        </ScrollArea>
      )}
      {notes.loaded && notes.error !== null ? (
        <Alert className="mx-4 mb-3">{notes.error}</Alert>
      ) : null}
    </SidePanelFrame>
  );
});
