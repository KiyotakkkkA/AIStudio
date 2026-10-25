import { useEffect } from "react";
import { observer } from "mobx-react-lite";
import {
  mdiArrowLeft,
  mdiConsole,
  mdiCreationOutline,
  mdiDeleteOutline,
  mdiFolderOutline,
  mdiPlus,
  mdiStop,
  mdiTuneVariant,
} from "@mdi/js";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import Alert from "../../ui/molecules/Alert";
import EmptyState from "../../ui/molecules/EmptyState";
import PageHeader from "../../ui/organisms/PageHeader";
import { useStore } from "../../stores/useStore";
import { formatLastUsed } from "../tools/toolPresentation";
import PanelButton from "../chat/PanelButton";
import ChatExplorer from "../chat/explorer/ChatExplorer";
import ExplorerFileView from "../chat/explorer/ExplorerFileView";
import { FILES_PANEL, PANEL_GROUP_LABEL } from "../chat/explorer/explorerCopy";
import TerminalDock from "../terminal/TerminalDock";
import { TERMINAL_PANEL } from "../terminal/terminalCopy";
import ViewerDock from "../../viewer/ViewerDock";
import {
  BACK_TO_LIST,
  BACK_TO_SESSION,
  EMPTY_DESCRIPTION,
  EMPTY_TITLE,
  LOADING,
  NEW_SESSION,
  PAGE_TITLE,
  PHASE_LABELS,
  PHASE_TONES,
  progressLabel,
  REMOVE_SESSION,
  removeLead,
  SESSION_TITLE,
  sessionsWord,
  STOP_SESSION,
} from "./agenticCopy";
import type AgenticStore from "./AgenticStore";
import { isFinished } from "./AgenticStore";
import ContextRail from "./ContextRail";
import NewSessionModal from "./NewSessionModal";
import PlanColumn from "./PlanColumn";
import SessionStream from "./SessionStream";
import { planProgress, planRows, sessionPhase } from "./sessionModel";

const NewSessionButton = observer(function NewSessionButton({
  store,
}: {
  readonly store: AgenticStore;
}) {
  return (
    <Button tone="primary" onClick={store.openForm}>
      <Icon path={mdiPlus} size={15} />
      {NEW_SESSION}
    </Button>
  );
});

const ErrorBar = observer(function ErrorBar({ store }: { readonly store: AgenticStore }) {
  if (store.error === null) return null;
  return (
    <Alert bar onDismiss={store.dismissError}>
      {store.error}
    </Alert>
  );
});

const SessionList = observer(function SessionList({ store }: { readonly store: AgenticStore }) {
  const now = Date.now();
  const sessions = store.ordered;
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <PageHeader
        icon={mdiCreationOutline}
        title={PAGE_TITLE}
        subtitle={store.loaded ? sessionsWord(sessions.length) : LOADING}
        actions={<NewSessionButton store={store} />}
      />
      <ErrorBar store={store} />
      {store.loaded && sessions.length === 0 ? (
        <div className="flex flex-1 items-center justify-center">
          <EmptyState
            icon={mdiCreationOutline}
            title={EMPTY_TITLE}
            description={EMPTY_DESCRIPTION}
            action={<NewSessionButton store={store} />}
          />
        </div>
      ) : (
        <ul className="m-0 flex min-h-0 flex-1 list-none flex-col gap-2 overflow-y-auto px-6 py-5">
          {sessions.map((session) => {
            const phase = sessionPhase(session, null);
            const progress = planProgress(planRows(session, false));
            return (
              <li
                key={session.id}
                className="flex items-center gap-3 rounded-card border border-line bg-main-850 px-4 py-3 shadow-card hover:border-line-strong"
              >
                <StatusDot tone={PHASE_TONES[phase]} title={PHASE_LABELS[phase]} />
                <button
                  type="button"
                  className="flex min-w-0 flex-1 cursor-pointer flex-col text-left"
                  onClick={() => void store.open(session.id)}
                >
                  <span className="truncate text-[13.5px] font-semibold text-main-50">
                    {session.goal}
                  </span>
                  <span className="truncate text-[11.5px] text-main-500">
                    {[
                      PHASE_LABELS[phase],
                      `${String(progress.done)} / ${String(progress.total)}`,
                      session.modelId,
                      formatLastUsed(session.updatedAt, now),
                    ].join(" · ")}
                  </span>
                </button>
                {session.status === "running" ? null : (
                  <Button
                    tone="danger"
                    aria-label={REMOVE_SESSION}
                    disabled={store.removingId === session.id}
                    needConfirm
                    modalSetup={{
                      title: `${REMOVE_SESSION}?`,
                      content: removeLead(session.goal),
                      tone: "danger",
                      confirmLabel: REMOVE_SESSION,
                    }}
                    onClick={() => void store.remove(session.id)}
                  >
                    <Icon path={mdiDeleteOutline} size={15} />
                  </Button>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
});

const SessionHeader = observer(function SessionHeader({ store }: { readonly store: AgenticStore }) {
  const session = store.session;
  const phase = store.phase;
  if (session === null || phase === null) return null;
  return (
    <header className="flex h-14 flex-none items-center gap-3 border-b border-line px-4.5">
      <IconButton
        path={mdiArrowLeft}
        label={BACK_TO_LIST}
        tooltip="bottom-left"
        iconSize={18}
        className="size-8"
        onClick={store.close}
      />
      <h1
        className="m-0 min-w-0 flex-initial truncate text-[14.5px] font-semibold text-main-50"
        title={session.goal}
      >
        {session.goal}
      </h1>
      <span
        className={`inline-flex h-5 flex-none items-center gap-1.5 rounded-full bg-main-700 px-2 text-[11px] font-medium ${STATUS_TONE_TEXT[PHASE_TONES[phase]]}`}
      >
        <StatusDot tone={PHASE_TONES[phase]} />
        {PHASE_LABELS[phase]}
        {store.progress.total > 0
          ? ` · ${progressLabel(store.progress.current, store.progress.total)}`
          : ""}
      </span>
      <span className="flex-1" />
      <div
        role="group"
        aria-label={PANEL_GROUP_LABEL}
        className="flex flex-none items-center gap-0.5 rounded-control bg-main-800 p-0.5"
      >
        <PanelButton
          label={SESSION_TITLE}
          path={mdiTuneVariant}
          active={store.isPanelShown("session")}
          onClick={() => store.showPanel("session")}
        />
        <PanelButton
          label={FILES_PANEL}
          path={mdiFolderOutline}
          active={store.isPanelShown("files")}
          onClick={() => store.showPanel("files")}
        />
      </div>
      <div
        role="group"
        aria-label={PANEL_GROUP_LABEL}
        className="flex flex-none items-center gap-0.5 rounded-control bg-main-800 p-0.5"
      >
        <PanelButton
          label={TERMINAL_PANEL}
          path={mdiConsole}
          active={store.terminal.open}
          activeClassName="bg-accent-soft text-accent-medium hover:bg-accent-soft hover:text-accent-light"
          onClick={() => void store.terminal.toggle()}
        />
      </div>
      {isFinished(session) ? (
        <Button
          tone="danger"
          aria-label={REMOVE_SESSION}
          title={REMOVE_SESSION}
          disabled={store.removingId === session.id}
          needConfirm
          modalSetup={{
            title: `${REMOVE_SESSION}?`,
            content: removeLead(session.goal),
            tone: "danger",
            confirmLabel: REMOVE_SESSION,
          }}
          onClick={() => void store.remove(session.id)}
        >
          <Icon path={mdiDeleteOutline} size={15} />
        </Button>
      ) : (
        <Button
          tone="danger"
          aria-label={STOP_SESSION}
          title={STOP_SESSION}
          disabled={store.stopping}
          onClick={() => void store.cancel()}
        >
          <Icon path={mdiStop} size={15} />
        </Button>
      )}
      <NewSessionButton store={store} />
    </header>
  );
});

const SessionView = observer(function SessionView({ store }: { readonly store: AgenticStore }) {
  const { viewer } = useStore();
  const owner = store.explorerOwner;
  useEffect(() => {
    void store.terminal.show(owner);
  }, [store, owner]);
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <SessionHeader store={store} />
      <ErrorBar store={store} />
      {store.session === null ? (
        <div className="flex flex-1 items-center justify-center text-[12.5px] text-main-500">
          {LOADING}
        </div>
      ) : (
        <div className="relative flex min-h-0 flex-1 overflow-hidden">
          <PlanColumn store={store} />
          <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            <SessionStream store={store} />
            <TerminalDock store={store.terminal} available={store.session !== null} />
          </div>
          {viewer.isOpen ? (
            <ViewerDock />
          ) : store.isPanelShown("files") ? (
            <ExplorerFileView explorer={store.explorer} backLabel={BACK_TO_SESSION} />
          ) : null}
          {store.activePanel === "files" ? (
            <div className={store.panelOpen ? "flex border-l border-line" : "hidden"}>
              <ChatExplorer store={store} />
            </div>
          ) : store.panelOpen ? (
            <ContextRail store={store} />
          ) : null}
        </div>
      )}
    </div>
  );
});

export default observer(function AgenticWorkspace() {
  const { agentic: store } = useStore();
  useEffect(() => {
    void store.mount();
    return store.unmount;
  }, [store]);
  return (
    <>
      {store.activeId === null ? <SessionList store={store} /> : <SessionView store={store} />}
      <NewSessionModal store={store} />
    </>
  );
});
