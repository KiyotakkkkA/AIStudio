import { useState } from "react";
import { observer } from "mobx-react-lite";
import { mdiCheck, mdiClose, mdiDeleteOutline, mdiPencilOutline } from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import { toolTitle } from "../chat/toolCallCopy";
import {
  callsWord,
  DELETE_STEP,
  EDIT_STEP,
  PLAN_EMPTY,
  PLAN_HINT,
  PLAN_PENDING,
  PLAN_TITLE,
  STEP_TITLE_LABEL,
  STEP_WORKING,
} from "./agenticCopy";
import type AgenticStore from "./AgenticStore";
import type { PlanRow } from "./sessionModel";

function IndexBadge({ row, number }: { readonly row: PlanRow; readonly number: number }) {
  const base =
    "flex size-4.5 flex-none items-center justify-center rounded-full font-mono text-[9.5px] font-semibold";
  if (row.status === "done") {
    return (
      <span className={`${base} bg-ok-soft text-ok`}>
        <Icon path={mdiCheck} size={11} />
      </span>
    );
  }
  if (row.status === "failed") {
    return (
      <span className={`${base} bg-err-soft text-err`}>
        <Icon path={mdiClose} size={11} />
      </span>
    );
  }
  if (row.status === "running") {
    return <span className={`${base} bg-accent-dark text-on-accent`}>{number}</span>;
  }
  return <span className={`${base} border-[1.5px] border-main-600 text-main-500`}>{number}</span>;
}

const TITLE_TONE: Record<PlanRow["status"], string> = {
  done: "text-main-300",
  failed: "text-main-300",
  running: "font-semibold text-main-50",
  queued: "text-main-400",
};

const StepMeta = observer(function StepMeta({
  store,
  row,
}: {
  readonly store: AgenticStore;
  readonly row: PlanRow;
}) {
  if (row.status === "running") {
    const call = store.activeCall;
    return (
      <p className="m-0 mt-0.5 truncate font-mono text-[10.5px] text-accent-medium">
        {call === null ? `${STEP_WORKING}…` : `${toolTitle(call.type)}…`}
      </p>
    );
  }
  if (row.status === "queued") return null;
  const meta = row.summary ?? (row.calls > 0 ? callsWord(row.calls) : null);
  return meta === null ? null : (
    <p className="m-0 mt-0.5 line-clamp-2 text-[11px] text-main-500" title={meta}>
      {meta}
    </p>
  );
});

function StepEditor({
  title,
  onSave,
  onCancel,
}: {
  readonly title: string;
  readonly onSave: (title: string) => void;
  readonly onCancel: () => void;
}) {
  const [value, setValue] = useState(title);
  const save = () => {
    if (value.trim().length === 0 || value.trim() === title) onCancel();
    else onSave(value);
  };
  return (
    <input
      aria-label={STEP_TITLE_LABEL}
      autoFocus
      value={value}
      maxLength={200}
      className="w-full min-w-0 rounded-control border border-accent-dark bg-main-900 px-1.5 py-0.5 text-[12.5px] text-main-50 outline-none"
      onChange={(event) => {
        setValue(event.target.value);
      }}
      onBlur={save}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          save();
        }
        if (event.key === "Escape") {
          event.preventDefault();
          onCancel();
        }
      }}
    />
  );
}

export default observer(function PlanColumn({ store }: { readonly store: AgenticStore }) {
  const [editing, setEditing] = useState<string | null>(null);
  const [dragged, setDragged] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);
  const rows = store.rows;
  const progress = store.progress;
  const fixed = rows.filter((row) => row.status !== "queued").length;
  const pending = store.session?.pendingPlan !== null && store.session?.pendingPlan !== undefined;
  const width = progress.total === 0 ? 0 : (progress.done / progress.total) * 100;

  const finishDrag = () => {
    setDragged(null);
    setOver(null);
  };

  return (
    <aside
      aria-label={PLAN_TITLE}
      className="flex w-74 flex-none flex-col border-r border-line bg-main-800"
    >
      <div className="flex items-center gap-2 px-4 pt-4 pb-2.5">
        <span className="flex-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          {PLAN_TITLE}
        </span>
        <span className="font-mono text-[10.5px] text-main-500 tabular-nums">
          {progress.done} / {progress.total}
        </span>
      </div>
      <div className="px-4 pb-2">
        <div className="h-0.75 rounded-full bg-main-700">
          <div
            className="h-full rounded-full bg-accent-dark transition-[width]"
            style={{ width: `${String(width)}%` }}
          />
        </div>
      </div>
      {rows.length === 0 ? (
        <p className="m-0 p-4 text-[12px] text-main-500">{PLAN_EMPTY}</p>
      ) : (
        <ScrollArea className="flex min-h-0 flex-1 flex-col gap-0.5 px-2 pb-2">
          {rows.map((row, index) => {
            const queuedIndex = index - fixed;
            const draggable = row.editable && editing === null && !store.planBusy;
            return (
              <div
                key={row.key}
                draggable={draggable}
                onDragStart={(event) => {
                  event.dataTransfer.effectAllowed = "move";
                  setDragged(queuedIndex);
                }}
                onDragOver={(event) => {
                  if (dragged === null || !row.editable) return;
                  event.preventDefault();
                  setOver(queuedIndex);
                }}
                onDrop={(event) => {
                  event.preventDefault();
                  if (dragged !== null && row.editable) void store.moveStep(dragged, queuedIndex);
                  finishDrag();
                }}
                onDragEnd={finishDrag}
                className={`group flex gap-2.5 rounded-control px-2.5 py-2 ${
                  row.status === "running"
                    ? "bg-main-900 shadow-[inset_2px_0_0_var(--color-accent-dark)]"
                    : ""
                } ${draggable ? "cursor-grab active:cursor-grabbing" : ""} ${
                  over === queuedIndex && dragged !== null && dragged !== queuedIndex
                    ? "shadow-[inset_0_2px_0_var(--color-accent-dark)]"
                    : ""
                } ${dragged === queuedIndex ? "opacity-50" : ""}`}
              >
                <span className="pt-px">
                  <IndexBadge row={row} number={index + 1} />
                </span>
                <div className="min-w-0 flex-1">
                  {editing === row.key ? (
                    <StepEditor
                      title={row.title}
                      onCancel={() => {
                        setEditing(null);
                      }}
                      onSave={(title) => {
                        setEditing(null);
                        void store.renameStep(queuedIndex, title);
                      }}
                    />
                  ) : (
                    <p className={`m-0 text-[12.5px] leading-snug ${TITLE_TONE[row.status]}`}>
                      {row.title}
                    </p>
                  )}
                  <StepMeta store={store} row={row} />
                </div>
                {row.editable && editing !== row.key ? (
                  <div className="flex flex-none items-start gap-0.5 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100">
                    <IconButton
                      path={mdiPencilOutline}
                      label={EDIT_STEP}
                      iconSize={13}
                      className="size-6"
                      disabled={store.planBusy}
                      onClick={() => {
                        setEditing(row.key);
                      }}
                    />
                    <IconButton
                      path={mdiDeleteOutline}
                      label={DELETE_STEP}
                      iconSize={13}
                      className="size-6"
                      disabled={store.planBusy}
                      onClick={() => void store.deleteStep(queuedIndex)}
                    />
                  </div>
                ) : null}
              </div>
            );
          })}
        </ScrollArea>
      )}
      {store.editable ? (
        <p className="m-0 mx-3 mb-3 mt-auto rounded-card bg-main-900 p-3 text-[11.5px] leading-relaxed text-main-400 shadow-card">
          {pending ? PLAN_PENDING : PLAN_HINT}
        </p>
      ) : null}
    </aside>
  );
});
