import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  mdiAlertCircleOutline,
  mdiCancel,
  mdiCheck,
  mdiChevronDown,
  mdiChevronUp,
  mdiClockOutline,
  mdiLoading,
  mdiMonitor,
  mdiSendOutline,
  mdiStop,
} from "@mdi/js";
import type {
  ChatToolCallStatus,
  ComputerAction,
  ComputerApproval,
  ComputerAppDto,
  ComputerCall,
  ComputerCommand,
  ComputerPointer,
  ComputerRequest,
  ComputerState,
} from "@zvs/shared";
import "../renderer/app/theme.css";
import "./style.css";
import Icon from "../browser-ui/Icon";
import { applyChromeAppearance } from "../browser-ui/appearance";
import { useIconUrl } from "../renderer/features/computer/iconUrl";
import { toolPresentation } from "../renderer/features/chat/toolPresentation";
import { GLOW_LABEL, sendCommand, subscribePointer, subscribeState, windowLabel } from "./bridge";
import {
  ACCESS_ANSWERS,
  ACCESS_TITLE,
  ACTION_STATE_TEXT,
  ACTION_TITLE,
  ACTIONS_EMPTY,
  ACTIONS_TITLE,
  ALLOW,
  BADGE_TEXT,
  CALL_STATUS_TEXT,
  CALLS_EMPTY,
  CALLS_TITLE,
  COLLAPSE,
  COMMAND_FAILED,
  COMPOSER_PLACEHOLDER,
  DENY,
  ELAPSED_TITLE,
  EXPAND,
  FOLDER_ANSWERS,
  FOLDER_REQUEST_KIND,
  PANEL_TITLE,
  REPLY_EMPTY,
  REPLY_TITLE,
  SEND,
  STATUS_TEXT,
  STOP,
  STOP_HINT,
  elapsedText,
  requestTitle,
} from "./computerCopy";

const DARK_SCHEME_QUERY = "(prefers-color-scheme: dark)";
const ACTIONS_SHOWN = 8;
const CALLS_SHOWN = 12;

const INITIAL: ComputerState = {
  active: false,
  status: "idle",
  apps: [],
  actions: [],
  collapsed: false,
  appearance: { theme: "system", darkPalette: "zvs-dark", lightPalette: "zvs-light" },
  reply: "",
  calls: [],
  requests: [],
};

const ACTION_ICONS: Record<ComputerAction["state"], string> = {
  running: mdiLoading,
  waiting: mdiClockOutline,
  done: mdiCheck,
  failed: mdiAlertCircleOutline,
  denied: mdiCancel,
};

const CALL_ICONS: Record<ChatToolCallStatus, string> = {
  running: mdiLoading,
  "awaiting-approval": mdiClockOutline,
  "awaiting-answer": mdiClockOutline,
  succeeded: mdiCheck,
  failed: mdiAlertCircleOutline,
  refused: mdiAlertCircleOutline,
  denied: mdiCancel,
  cancelled: mdiCancel,
};

const CALL_TONE: Record<ChatToolCallStatus, ComputerAction["state"]> = {
  running: "running",
  "awaiting-approval": "waiting",
  "awaiting-answer": "waiting",
  succeeded: "done",
  failed: "failed",
  refused: "failed",
  denied: "denied",
  cancelled: "denied",
};

function useOsDark(): boolean {
  const [dark, setDark] = useState(() => window.matchMedia(DARK_SCHEME_QUERY).matches);
  useEffect(() => {
    const list = window.matchMedia(DARK_SCHEME_QUERY);
    const sync = () => setDark(list.matches);
    list.addEventListener("change", sync);
    return () => list.removeEventListener("change", sync);
  }, []);
  return dark;
}

function useComputerState(): ComputerState {
  const [state, setState] = useState<ComputerState>(INITIAL);
  const osDark = useOsDark();
  useEffect(() => subscribeState(setState), []);
  useEffect(() => applyChromeAppearance(state.appearance, osDark), [state.appearance, osDark]);
  return state;
}

function useNow(ticking: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!ticking) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [ticking]);
  return now;
}

function AppRow({ app }: { readonly app: ComputerAppDto }) {
  const url = useIconUrl(app.icon);
  const [failed, setFailed] = useState<string | null>(null);
  return (
    <li className="app-row">
      {url === null || failed === url ? (
        <span className="app-icon app-icon-empty">
          <Icon path={mdiMonitor} size={16} />
        </span>
      ) : (
        <img className="app-icon" src={url} alt="" onError={() => setFailed(url)} />
      )}
      <span className="app-text">
        <span className="app-name">{app.app}</span>
        {app.path.length > 0 ? (
          <span className="app-path" title={app.path}>
            {app.path}
          </span>
        ) : null}
      </span>
    </li>
  );
}

function ApprovalBlock({
  approval,
  run,
}: {
  readonly approval: ComputerApproval;
  readonly run: (command: ComputerCommand) => void;
}) {
  const access = approval.kind === "access";
  return (
    <section className="approval" aria-label={access ? ACCESS_TITLE : ACTION_TITLE}>
      <h2>{access ? ACCESS_TITLE : ACTION_TITLE}</h2>
      {approval.detail === undefined ? null : <p className="approval-detail">{approval.detail}</p>}
      {approval.apps.length === 0 ? null : (
        <ul className="app-list">
          {approval.apps.map((app) => (
            <AppRow key={app.app} app={app} />
          ))}
        </ul>
      )}
      <div className="approval-actions">
        {access ? (
          ACCESS_ANSWERS.map(({ answer, label }) => (
            <button
              key={answer}
              type="button"
              className={answer === "deny" ? "danger-soft" : answer === "once" ? "primary" : ""}
              onClick={() => run({ action: "answer", id: approval.id, answer })}
            >
              {label}
            </button>
          ))
        ) : (
          <>
            <button
              type="button"
              className="primary"
              onClick={() => run({ action: "approve", id: approval.id })}
            >
              {ALLOW}
            </button>
            <button
              type="button"
              className="danger-soft"
              onClick={() => run({ action: "deny", id: approval.id })}
            >
              {DENY}
            </button>
          </>
        )}
      </div>
    </section>
  );
}

function RequestBlock({
  request,
  run,
}: {
  readonly request: ComputerRequest;
  readonly run: (command: ComputerCommand) => void;
}) {
  const title = requestTitle(request.kind);
  return (
    <section className="approval" aria-label={title}>
      <h2>{title}</h2>
      {request.reason === undefined ? null : <p className="approval-reason">{request.reason}</p>}
      {request.detail === undefined ? null : <pre className="approval-code">{request.detail}</pre>}
      <p className="approval-tool">{toolPresentation(request.tool).title}</p>
      <div className="approval-actions">
        {request.kind === FOLDER_REQUEST_KIND ? (
          FOLDER_ANSWERS.map(({ answer, label }) => (
            <button
              key={answer}
              type="button"
              className={answer === "deny" ? "danger-soft" : answer === "once" ? "primary" : ""}
              onClick={() => run({ action: "answer", id: request.id, answer })}
            >
              {label}
            </button>
          ))
        ) : (
          <>
            <button
              type="button"
              className="primary"
              onClick={() => run({ action: "approve", id: request.id })}
            >
              {ALLOW}
            </button>
            <button
              type="button"
              className="danger-soft"
              onClick={() => run({ action: "deny", id: request.id })}
            >
              {DENY}
            </button>
          </>
        )}
      </div>
    </section>
  );
}

function Reply({ text, live }: { readonly text: string; readonly live: boolean }) {
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = box.current;
    if (element !== null) element.scrollTop = element.scrollHeight;
  }, [text]);
  return (
    <section className="section" aria-label={REPLY_TITLE}>
      <h3 className="section-title">{REPLY_TITLE}</h3>
      <div ref={box} className={`reply ${live ? "reply-live" : ""}`} aria-live="polite">
        {text.trim().length === 0 ? <span className="reply-empty">{REPLY_EMPTY}</span> : text}
      </div>
    </section>
  );
}

function CallRow({ call }: { readonly call: ComputerCall }) {
  const tone = CALL_TONE[call.status];
  return (
    <li className={`action action-${tone}`}>
      <span className="action-mark" title={CALL_STATUS_TEXT[call.status]}>
        <Icon path={CALL_ICONS[call.status]} size={14} />
      </span>
      <span className="action-text">
        <span className="action-label">{toolPresentation(call.key).title}</span>
        {call.summary === undefined ? null : <span className="action-summary">{call.summary}</span>}
      </span>
      <span className="action-state">{CALL_STATUS_TEXT[call.status]}</span>
    </li>
  );
}

function ActionRow({ action }: { readonly action: ComputerAction }) {
  return (
    <li className={`action action-${action.state}`}>
      <span className="action-mark" title={ACTION_STATE_TEXT[action.state]}>
        <Icon path={ACTION_ICONS[action.state]} size={14} />
      </span>
      <span className="action-text">
        <span className="action-label">{action.label}</span>
        {action.error === undefined ? null : <span className="action-error">{action.error}</span>}
      </span>
    </li>
  );
}

function Composer({ run }: { readonly run: (command: ComputerCommand) => void }) {
  const [text, setText] = useState("");
  const field = useRef<HTMLTextAreaElement>(null);
  const submit = () => {
    const value = text.trim();
    if (value.length === 0) return;
    setText("");
    field.current?.blur();
    run({ action: "say", text: value });
  };
  return (
    <form
      className="composer"
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      <textarea
        ref={field}
        rows={2}
        value={text}
        placeholder={COMPOSER_PLACEHOLDER}
        onPointerDown={() => run({ action: "engage" })}
        onFocus={() => run({ action: "engage" })}
        onBlur={() => run({ action: "release" })}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault();
            submit();
          }
          if (event.key === "Escape") field.current?.blur();
        }}
      />
      <button type="submit" className="icon-button" aria-label={SEND} disabled={text.trim() === ""}>
        <Icon path={mdiSendOutline} size={16} />
      </button>
    </form>
  );
}

function Panel({ state }: { readonly state: ComputerState }) {
  const [error, setError] = useState<string | null>(null);
  const live = state.active && state.status !== "idle" && state.status !== "stopping";
  const now = useNow(live && state.startedAt !== undefined);
  const run = (command: ComputerCommand) => {
    setError(null);
    sendCommand(command).catch((reason: unknown) => {
      setError(typeof reason === "string" ? reason : COMMAND_FAILED);
    });
  };
  const actions = state.actions.slice(-ACTIONS_SHOWN).reverse();
  const calls = state.calls.slice(-CALLS_SHOWN).reverse();
  const elapsed = elapsedText(state.startedAt, now);
  return (
    <main className={`panel ${state.collapsed ? "collapsed" : ""}`}>
      <header className="panel-header">
        <span className={`pulse status-${state.status}`} aria-hidden="true" />
        <span className="panel-heading">
          <span className="panel-title">{PANEL_TITLE}</span>
          <span className="panel-subtitle">{state.title ?? STATUS_TEXT[state.status]}</span>
        </span>
        {elapsed === null ? null : (
          <span className="elapsed" title={ELAPSED_TITLE}>
            {elapsed}
          </span>
        )}
        <button
          type="button"
          className="icon-button"
          aria-label={state.collapsed ? EXPAND : COLLAPSE}
          onClick={() => run({ action: "collapse", collapsed: !state.collapsed })}
        >
          <Icon path={state.collapsed ? mdiChevronUp : mdiChevronDown} size={16} />
        </button>
        <button type="button" className="danger" onClick={() => run({ action: "stop" })}>
          <Icon path={mdiStop} size={14} />
          {STOP}
        </button>
      </header>
      {state.collapsed ? null : (
        <>
          <p className={`hint ${live ? "zvs-shimmer-text" : ""}`}>
            {STATUS_TEXT[state.status]} · {STOP_HINT}
          </p>
          <div className="panel-body">
            {state.approval === undefined ? null : (
              <ApprovalBlock approval={state.approval} run={run} />
            )}
            {state.requests.map((request) => (
              <RequestBlock key={request.id} request={request} run={run} />
            ))}
            <Reply text={state.reply} live={live} />
            <section className="section" aria-label={CALLS_TITLE}>
              <h3 className="section-title">{CALLS_TITLE}</h3>
              <ol className="actions" aria-live="polite">
                {calls.length === 0 ? <li className="actions-empty">{CALLS_EMPTY}</li> : null}
                {calls.map((call) => (
                  <CallRow key={call.id} call={call} />
                ))}
              </ol>
            </section>
            <section className="section" aria-label={ACTIONS_TITLE}>
              <h3 className="section-title">{ACTIONS_TITLE}</h3>
              <ol className="actions">
                {actions.length === 0 ? <li className="actions-empty">{ACTIONS_EMPTY}</li> : null}
                {actions.map((action) => (
                  <ActionRow key={action.id} action={action} />
                ))}
              </ol>
            </section>
          </div>
          {error === null ? null : <p className="panel-error">{error}</p>}
          <Composer run={run} />
        </>
      )}
    </main>
  );
}

function usePointer(): ComputerPointer | null {
  const [pointer, setPointer] = useState<ComputerPointer | null>(null);
  useEffect(() => subscribePointer(setPointer), []);
  return pointer;
}

function AgentPointer() {
  const pointer = usePointer();
  const [clicks, setClicks] = useState(0);
  const pressed = useRef(false);
  useEffect(() => {
    if (pointer === null) return;
    if (pointer.down && !pressed.current) setClicks((count) => count + 1);
    pressed.current = pointer.down;
  }, [pointer]);
  if (pointer === null) return null;
  const scale = window.devicePixelRatio || 1;
  const x = pointer.x / scale;
  const y = pointer.y / scale;
  if (x < 0 || y < 0 || x > window.innerWidth || y > window.innerHeight) return null;
  return (
    <div
      className={`agent-pointer ${pointer.down ? "agent-pointer-down" : ""}`}
      style={{ transform: `translate(${x}px, ${y}px)` }}
      aria-hidden="true"
    >
      {clicks > 0 ? <span key={clicks} className="agent-ripple" /> : null}
      <svg className="agent-arrow" width="26" height="26" viewBox="0 0 24 24">
        <path d="M4 2.5 19.5 12 12.6 13.4 16.4 20.8 13.5 22.2 9.7 14.8 4.6 19.5Z" />
      </svg>
    </div>
  );
}

function Glow({ state }: { readonly state: ComputerState }) {
  return (
    <div className={`glow status-${state.status}`}>
      <div className="badge" role="status">
        <span className={`pulse status-${state.status}`} aria-hidden="true" />
        <span className="badge-text">{BADGE_TEXT}</span>
        <span className="badge-status">{STATUS_TEXT[state.status]}</span>
        <kbd>{STOP_HINT}</kbd>
      </div>
      <AgentPointer />
    </div>
  );
}

function ComputerApp() {
  const state = useComputerState();
  return windowLabel() === GLOW_LABEL ? <Glow state={state} /> : <Panel state={state} />;
}

createRoot(document.getElementById("root")!).render(<ComputerApp />);
