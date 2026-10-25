import { useCallback, useRef, useState, type PointerEvent } from "react";
import { observer } from "mobx-react-lite";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import type { Terminal } from "@xterm/xterm";
import type { TerminalSessionDto } from "@zvs/shared";
import {
  mdiChevronDown,
  mdiClose,
  mdiContentCopy,
  mdiEraser,
  mdiPlus,
  mdiRestart,
  mdiStopCircleOutline,
} from "@mdi/js";
import Icon from "../../ui/atoms/Icon";
import IconButton from "../../ui/atoms/buttons/IconButton";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import StatusDot from "../../ui/atoms/StatusDot";
import type TerminalStore from "./TerminalStore";
import TerminalView, { copySelection } from "./TerminalView";
import {
  CLEAR,
  CLOSE_SESSION,
  COPY,
  DISMISS_ERROR,
  DOCK_LABEL,
  EMPTY,
  EMPTY_HINT,
  HIDE_DOCK,
  INTERRUPT,
  NEW_SESSION,
  NEW_SESSION_WITH,
  NO_DIALOG,
  RENAME_SESSION,
  RESIZE_DOCK,
  RESTART,
  SHELL_LABELS,
  STARTING,
  STATUS_RUNNING,
  TABS_LABEL,
  exitLabel,
} from "./terminalCopy";
import Alert from "../../ui/molecules/Alert";

const THREAD_MIN = 220;

function statusOf(session: TerminalSessionDto): { tone: "ok" | "idle" | "err"; label: string } {
  if (session.status === "running") return { tone: "ok", label: STATUS_RUNNING };
  return {
    tone: session.exitCode === null || session.exitCode === 0 ? "idle" : "err",
    label: exitLabel(session.exitCode),
  };
}

const SessionTab = observer(function SessionTab({
  store,
  session,
}: {
  readonly store: TerminalStore;
  readonly session: TerminalSessionDto;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(session.title);
  const active = store.activeId === session.id;
  const status = statusOf(session);
  const commit = () => {
    setEditing(false);
    if (draft.trim() !== "" && draft.trim() !== session.title) void store.rename(session.id, draft);
  };
  return (
    <div
      role="tab"
      aria-selected={active}
      tabIndex={active ? 0 : -1}
      onClick={() => store.select(session.id)}
      onDoubleClick={() => {
        setDraft(session.title);
        setEditing(true);
      }}
      title={`${session.title} · ${status.label}\n${session.cwd}`}
      className={`group flex h-7 max-w-48 min-w-0 flex-none cursor-default items-center gap-1.5 rounded-control pr-1 pl-2.5 text-[12px] ${active ? "bg-main-700 text-main-50" : "text-main-400 hover:bg-main-800 hover:text-main-100"}`}
    >
      <StatusDot tone={status.tone} title={status.label} />
      {editing ? (
        <input
          aria-label={RENAME_SESSION}
          autoFocus
          value={draft}
          maxLength={80}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") commit();
            if (event.key === "Escape") setEditing(false);
          }}
          className="w-28 min-w-0 rounded-sm bg-main-900 px-1 text-[12px] text-main-50 outline-none"
        />
      ) : (
        <span className="truncate">{session.title}</span>
      )}
      <button
        type="button"
        aria-label={`${CLOSE_SESSION}: ${session.title}`}
        onClick={(event) => {
          event.stopPropagation();
          void store.close(session.id);
        }}
        className={`flex size-5 flex-none items-center justify-center rounded-sm text-main-500 hover:bg-main-600 hover:text-main-50 ${active ? "" : "opacity-0 group-hover:opacity-100"}`}
      >
        <Icon path={mdiClose} size={13} />
      </button>
    </div>
  );
});

export default observer(function TerminalDock({
  store,
  available,
}: {
  readonly store: TerminalStore;
  readonly available: boolean;
}) {
  const terminal = useRef<Terminal | null>(null);
  const onReady = useCallback((instance: Terminal | null) => {
    terminal.current = instance;
  }, []);
  if (!store.open) return null;
  const active = store.active;

  const startResize = (event: PointerEvent<HTMLDivElement>) => {
    const handle = event.currentTarget;
    const dock = handle.parentElement;
    const thread = dock?.parentElement;
    if (dock === null || thread === null || thread === undefined) return;
    handle.setPointerCapture(event.pointerId);
    const bottom = dock.getBoundingClientRect().bottom;
    const limit = thread.getBoundingClientRect().height - THREAD_MIN;
    const move = (next: globalThis.PointerEvent) => {
      store.setHeight(Math.min(limit, bottom - next.clientY));
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
      aria-label={DOCK_LABEL}
      style={{ height: `${String(store.height)}px` }}
      className="relative flex min-h-0 flex-none flex-col border-t border-line bg-main-900"
    >
      <div
        role="separator"
        aria-orientation="horizontal"
        aria-label={RESIZE_DOCK}
        onPointerDown={startResize}
        className="absolute inset-x-0 -top-1 z-10 h-2 cursor-row-resize hover:bg-accent-line"
      />
      <div className="flex h-10 flex-none items-center gap-1 border-b border-line px-3">
        <div
          role="tablist"
          aria-label={TABS_LABEL}
          className="flex min-w-0 flex-1 items-center gap-1 scrollbar-none overflow-x-auto"
        >
          {store.sessions.map((session) => (
            <SessionTab key={session.id} store={store} session={session} />
          ))}
          {available ? (
            <div className="flex flex-none items-center">
              <IconButton
                label={NEW_SESSION}
                path={mdiPlus}
                iconSize={16}
                disabled={store.creating}
                onClick={() => void store.create(null)}
              />
              <Dropdown
                menuWidth={220}
                menuPlacement="bottom-left"
                onOpenChange={(open) => {
                  if (open && store.shells.length === 0) void store.loadShells();
                }}
              >
                <IconDropdownButton
                  label={NEW_SESSION_WITH}
                  path={mdiChevronDown}
                  iconSize={14}
                  rounded="rounded-lg"
                  disabled={store.creating}
                  className="size-6 justify-center gap-0 border-transparent bg-transparent p-0 text-main-400 hover:bg-main-700 hover:text-main-50"
                />
                <Dropdown.Menu
                  role="menu"
                  aria-label={NEW_SESSION_WITH}
                  rounded="rounded-lg"
                  className="rounded-card border-line bg-main-900 p-1 shadow-pop"
                >
                  {store.shells.map((shell) => (
                    <Dropdown.Item
                      key={shell}
                      role="menuitem"
                      rounded="rounded-lg"
                      className="text-[13px] text-main-100"
                      onClick={() => void store.create(shell)}
                    >
                      {SHELL_LABELS[shell]}
                    </Dropdown.Item>
                  ))}
                </Dropdown.Menu>
              </Dropdown>
            </div>
          ) : null}
        </div>
        {active === null ? null : (
          <div className="flex flex-none items-center gap-0.5">
            <IconButton
              label={INTERRUPT}
              path={mdiStopCircleOutline}
              disabled={active.status !== "running"}
              onClick={() => store.interrupt(active.id)}
            />
            <IconButton
              label={COPY}
              path={mdiContentCopy}
              onClick={() => {
                if (terminal.current !== null) copySelection(terminal.current);
              }}
            />
            <IconButton
              label={CLEAR}
              path={mdiEraser}
              onClick={() => void store.clear(active.id)}
            />
            <IconButton
              label={RESTART}
              path={mdiRestart}
              onClick={() => void store.restart(active.id)}
            />
          </div>
        )}
        <IconButton label={HIDE_DOCK} path={mdiClose} iconSize={16} onClick={store.hide} />
      </div>
      {store.error === null ? null : (
        <Alert bar onDismiss={store.dismissError} dismissLabel={DISMISS_ERROR}>
          {store.error}
        </Alert>
      )}
      <div className="relative min-h-0 flex-1">
        {!available ? (
          <p className="px-4 py-3 text-[12.5px] text-main-500">{NO_DIALOG}</p>
        ) : active !== null ? (
          <>
            <TerminalView key={active.id} store={store} id={active.id} onReady={onReady} />
            {active.status === "exited" ? (
              <div className="absolute right-3 bottom-2 flex items-center gap-2 rounded-control bg-main-800 py-1 pr-1 pl-3 text-[12px] text-main-300 shadow-card">
                {exitLabel(active.exitCode)}
                <button
                  type="button"
                  onClick={() => void store.restart(active.id)}
                  className="rounded-control px-2 py-0.5 text-accent-medium hover:bg-main-700"
                >
                  {RESTART}
                </button>
              </div>
            ) : null}
          </>
        ) : store.creating ? (
          <p role="status" className="px-4 py-3 text-[12.5px] text-main-500">
            {STARTING}
          </p>
        ) : (
          <div className="flex flex-col gap-1 px-4 py-3">
            <p className="text-[12.5px] text-main-300">{EMPTY}</p>
            <p className="text-[12px] text-main-500">{EMPTY_HINT}</p>
          </div>
        )}
      </div>
    </section>
  );
});
