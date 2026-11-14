import type { PointerEvent, ReactNode } from "react";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import {
  mdiArrowLeft,
  mdiClose,
  mdiDeleteOutline,
  mdiNoteAlertOutline,
  mdiNoteRemoveOutline,
  mdiNoteTextOutline,
  mdiPencilOutline,
  mdiRenameOutline,
} from "@mdi/js";
import Icon from "../../../ui/atoms/Icon";
import IconButton from "../../../ui/atoms/buttons/IconButton";
import Button from "../../../ui/atoms/buttons/Button";
import TextInput from "../../../ui/atoms/TextInput";
import Alert from "../../../ui/molecules/Alert";
import ConfirmDialog from "../../../ui/molecules/ConfirmDialog";
import EmptyState from "../../../ui/molecules/EmptyState";
import { DocumentSkeleton } from "../../../ui/molecules/LoadingSkeleton";
import InstructionEditor from "../../skills/InstructionEditor";
import ChatMarkdown from "../ChatMarkdown";
import { RESIZE_VIEW } from "../explorer/explorerCopy";
import type NotesStore from "./NotesStore";
import {
  NOTE_BACK,
  NOTE_CANCEL,
  NOTE_CLOSE,
  NOTE_CONTEXT,
  NOTE_CREATED,
  NOTE_DELETE,
  NOTE_DELETE_TITLE,
  NOTE_DELETE_WHEN,
  NOTE_EDIT,
  NOTE_EDITOR_LABEL,
  NOTE_LOADING,
  NOTE_READ_WHEN,
  NOTE_RELOAD,
  NOTE_REMOVED,
  NOTE_REMOVED_HINT,
  NOTE_RENAME,
  NOTE_RENAME_LABEL,
  NOTE_SAVE,
  NOTE_UPDATE_WHEN,
  NOTE_UPDATED,
  NOTE_VIEW_LABEL,
  noteDate,
  noteDeleteBody,
} from "./notesCopy";

function Field({ label, children }: { readonly label: string; readonly children: ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-[11px] font-medium text-main-500">{label}</dt>
      <dd className="m-0 text-[12.5px]/relaxed text-main-200">{children}</dd>
    </div>
  );
}

export default observer(function NoteView({ notes }: { readonly notes: NotesStore }) {
  if (!notes.viewOpen) return null;
  const note = notes.note;
  const meta = note?.meta ?? null;
  const editing = notes.mode === "edit";
  const renaming = notes.mode === "rename";

  const startResize = (event: PointerEvent<HTMLDivElement>) => {
    const handle = event.currentTarget;
    const view = handle.parentElement;
    if (view === null) return;
    handle.setPointerCapture(event.pointerId);
    const right = view.getBoundingClientRect().right;
    const thread = view.previousElementSibling?.getBoundingClientRect();
    const limit = thread === undefined ? Infinity : right - thread.left - 420;
    const move = (next: globalThis.PointerEvent) => {
      notes.setViewWidth(Math.min(limit, right - next.clientX));
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
      aria-label={NOTE_VIEW_LABEL}
      style={{ width: `${String(notes.viewWidth)}px` }}
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
            label={NOTE_BACK}
            tooltip="bottom-center"
            path={mdiArrowLeft}
            iconSize={17}
            onClick={notes.close}
          />
        </span>
        <Icon path={meta === null ? mdiNoteAlertOutline : mdiNoteTextOutline} size={15} />
        <div className="flex min-w-0 flex-1 flex-col">
          {renaming ? (
            <TextInput
              mono
              aria-label={NOTE_RENAME_LABEL}
              value={notes.renameDraft}
              invalid={notes.renameHint !== null}
              autoFocus
              onChange={(event) => notes.setRenameDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void notes.rename();
                if (event.key === "Escape") notes.cancel();
              }}
            />
          ) : (
            <>
              <span className="truncate text-[13px] font-medium text-main-100">
                {meta?.title ?? note?.name ?? NOTE_REMOVED}
              </span>
              {note === null ? null : (
                <span className="truncate font-mono text-[11px] text-main-500" title={note.path}>
                  {note.path}
                </span>
              )}
            </>
          )}
        </div>
        {note === null ? null : renaming ? (
          <>
            <Button size="sm" tone="ghost" disabled={notes.busy} onClick={notes.cancel}>
              {NOTE_CANCEL}
            </Button>
            <Button
              size="sm"
              tone="primary"
              disabled={notes.busy || !notes.renameValid}
              onClick={() => void notes.rename()}
            >
              {NOTE_SAVE}
            </Button>
          </>
        ) : editing ? (
          <>
            <Button size="sm" tone="ghost" disabled={notes.busy} onClick={notes.cancel}>
              {NOTE_CANCEL}
            </Button>
            <Button
              size="sm"
              tone="primary"
              disabled={notes.busy || !notes.dirty || notes.conflict}
              onClick={() => void notes.save()}
            >
              {NOTE_SAVE}
            </Button>
          </>
        ) : (
          <>
            <IconButton
              label={NOTE_EDIT}
              tooltip="bottom-center"
              path={mdiPencilOutline}
              iconSize={16}
              disabled={notes.busy}
              onClick={notes.edit}
            />
            <IconButton
              label={NOTE_RENAME}
              tooltip="bottom-center"
              path={mdiRenameOutline}
              iconSize={16}
              disabled={notes.busy}
              onClick={notes.startRename}
            />
            <IconButton
              label={NOTE_DELETE}
              tooltip="bottom-center"
              path={mdiDeleteOutline}
              iconSize={16}
              disabled={notes.busy}
              onClick={() => notes.requestRemove(note.name)}
            />
          </>
        )}
        <IconButton
          label={NOTE_CLOSE}
          tooltip="bottom-center"
          path={mdiClose}
          iconSize={17}
          onClick={notes.close}
        />
      </div>
      {notes.renameHint === null || !renaming ? null : (
        <p className="mx-4 mb-2 text-[11.5px] text-err">{notes.renameHint}</p>
      )}
      {notes.actionError === null ? null : (
        <Alert
          className="mx-4 mb-2"
          action={
            notes.conflict ? (
              <Button size="sm" onClick={() => void notes.reload()}>
                {NOTE_RELOAD}
              </Button>
            ) : undefined
          }
        >
          {notes.actionError}
        </Alert>
      )}
      <div className="flex min-h-0 flex-1 flex-col border-t border-line">
        {notes.removed ? (
          <div className="flex flex-1 items-center justify-center">
            <EmptyState
              icon={mdiNoteRemoveOutline}
              title={NOTE_REMOVED}
              description={NOTE_REMOVED_HINT}
            />
          </div>
        ) : note === null ? (
          notes.noteLoading ? (
            <DocumentSkeleton label={NOTE_LOADING} className="px-4 pt-3" />
          ) : notes.noteError === null ? null : (
            <div className="flex flex-1 items-center justify-center">
              <EmptyState
                icon={mdiNoteAlertOutline}
                title={notes.noteError}
                description={notes.openName ?? ""}
              />
            </div>
          )
        ) : editing ? (
          <div className="flex min-h-0 flex-1 flex-col p-3">
            <InstructionEditor
              value={notes.draft}
              fileName={note.name}
              label={NOTE_EDITOR_LABEL}
              invalid={notes.actionError !== null && !notes.conflict}
              onChange={notes.setDraft}
            />
          </div>
        ) : meta === null ? (
          <ScrollArea className="min-h-0 flex-1 px-4 pt-3 pb-8">
            {note.error === null ? null : <Alert className="mb-3">{note.error}</Alert>}
            <pre className="m-0 font-mono text-[12.5px]/relaxed whitespace-pre-wrap text-main-200">
              {note.content}
            </pre>
          </ScrollArea>
        ) : (
          <ScrollArea className="min-h-0 flex-1 px-6 pt-5 pb-8">
            <dl className="m-0 mb-5 grid gap-3 rounded-card border border-line bg-main-850 p-3.5">
              <Field label={NOTE_CONTEXT}>{meta.context}</Field>
              <Field label={NOTE_READ_WHEN}>{meta.readWhen}</Field>
              <Field label={NOTE_UPDATE_WHEN}>{meta.updateWhen}</Field>
              <Field label={NOTE_DELETE_WHEN}>{meta.deleteWhen}</Field>
              <div className="flex flex-wrap gap-x-5 gap-y-1 text-[11.5px] text-main-500">
                <span>
                  {NOTE_CREATED}: {noteDate(meta.createdAt) ?? meta.createdAt}
                </span>
                <span>
                  {NOTE_UPDATED}: {noteDate(meta.updatedAt) ?? meta.updatedAt}
                </span>
              </div>
            </dl>
            <ChatMarkdown content={note.body ?? ""} />
          </ScrollArea>
        )}
      </div>
      <ConfirmDialog
        open={notes.removing !== null}
        title={NOTE_DELETE_TITLE}
        confirmLabel={NOTE_DELETE}
        confirmTone="danger"
        onConfirm={() => void notes.confirmRemove()}
        onCancel={notes.cancelRemove}
      >
        {noteDeleteBody(notes.removing ?? "")}
      </ConfirmDialog>
    </section>
  );
});
