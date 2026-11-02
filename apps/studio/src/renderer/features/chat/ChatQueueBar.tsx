import { observer } from "mobx-react-lite";
import { mdiDeleteOutline, mdiPencilOutline, mdiTrayFull } from "@mdi/js";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import TextArea from "../../ui/atoms/TextArea";
import type ChatStore from "./ChatStore";
import {
  QUEUE_CANCEL,
  QUEUE_EDIT,
  QUEUE_EDIT_FIELD,
  QUEUE_LABEL,
  QUEUE_REMOVE,
  QUEUE_SAVE,
  QUEUE_SEND_NOW,
  QUEUE_SENDING,
  queuedWord,
} from "./queueCopy";

export default observer(function ChatQueueBar({ store }: { readonly store: ChatStore }) {
  const queue = store.queue;
  const messages = queue.messages;
  if (messages.length === 0) return null;
  const selected = queue.selected;

  return (
    <section aria-label={QUEUE_LABEL} className="px-3 py-2 text-[12.5px]">
      <div className="flex min-w-0 items-center gap-2">
        <Icon path={mdiTrayFull} size={16} className="shrink-0 text-accent-medium" />
        <span className="min-w-0 flex-1 truncate font-medium text-main-100">
          {QUEUE_LABEL} · {queuedWord(messages.length)}
        </span>
        <IconButton
          path={mdiPencilOutline}
          label={QUEUE_EDIT}
          tooltip="top-center"
          disabled={selected === null || queue.busy || queue.editing !== null}
          onClick={() => queue.startEdit()}
        />
        <IconButton
          path={mdiDeleteOutline}
          label={QUEUE_REMOVE}
          tooltip="top-center"
          tone="danger"
          disabled={selected === null || queue.busy}
          onClick={() => void queue.remove()}
        />
        <Button
          tone="primary"
          size="sm"
          disabled={queue.busy || queue.urgent}
          onClick={() => void queue.sendNow()}
        >
          {queue.urgent ? QUEUE_SENDING : QUEUE_SEND_NOW}
        </Button>
      </div>
      <ul role="listbox" aria-label={QUEUE_LABEL} className="mt-1.5 space-y-0.5">
        {messages.map((message) => {
          const active = message.id === queue.selectedId;
          if (message.id === queue.editingId)
            return (
              <li key={message.id} className="space-y-1.5 rounded-control bg-main-800 p-1.5">
                <TextArea
                  aria-label={QUEUE_EDIT_FIELD}
                  value={queue.draft}
                  minRows={1}
                  maxRows={6}
                  onChange={(event) => queue.setDraft(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.nativeEvent.isComposing) return;
                    if (event.key === "Enter" && !event.shiftKey) {
                      event.preventDefault();
                      void queue.saveEdit();
                    }
                    if (event.key === "Escape") {
                      event.preventDefault();
                      queue.cancelEdit();
                    }
                  }}
                />
                <div className="flex justify-end gap-1">
                  <Button tone="ghost" size="sm" onClick={queue.cancelEdit}>
                    {QUEUE_CANCEL}
                  </Button>
                  <Button
                    tone="primary"
                    size="sm"
                    disabled={queue.busy || queue.draft.trim().length === 0}
                    onClick={() => void queue.saveEdit()}
                  >
                    {QUEUE_SAVE}
                  </Button>
                </div>
              </li>
            );
          return (
            <li key={message.id} role="option" aria-selected={active}>
              <button
                type="button"
                title={message.text}
                onClick={() => queue.select(active ? null : message.id)}
                onDoubleClick={() => queue.startEdit(message.id)}
                className={`block w-full truncate rounded-control px-2 py-1 text-left ${
                  active ? "bg-accent-soft text-accent-medium" : "text-main-300 hover:bg-main-800"
                }`}
              >
                {message.text}
              </button>
            </li>
          );
        })}
      </ul>
      {queue.error === null ? null : (
        <p role="alert" className="mt-1 text-[11.5px] text-err">
          {queue.error}
        </p>
      )}
    </section>
  );
});
